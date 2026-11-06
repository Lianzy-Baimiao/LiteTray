use crate::{
    config::{UpdateConfig, validate_repository},
    http::JsonHttp,
};
use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Release {
    pub version: String,
    pub name: String,
    pub url: String,
    pub notes: String,
    pub published_at: String,
    pub prerelease: bool,
    pub tag_only: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UpdateStatus {
    Available(Release),
    UpToDate { current: String },
    NoReleases,
    RepositoryUnavailable(String),
}
impl UpdateStatus {
    pub fn text(&self) -> String {
        match self {
            Self::Available(r) => format!(
                "发现新版本 {}{}{}",
                r.version,
                if r.prerelease { "（预览版）" } else { "" },
                if r.tag_only {
                    " · 目前只有 Tag，尚无安装包"
                } else {
                    ""
                }
            ),
            Self::UpToDate { current } => format!("已是最新版本 {current}"),
            Self::NoReleases => "仓库已连接，尚未发布可用版本".into(),
            Self::RepositoryUnavailable(s) => s.clone(),
        }
    }
}
pub fn check(
    http: &dyn JsonHttp,
    config: &UpdateConfig,
    current: &str,
) -> Result<UpdateStatus, String> {
    validate_repository(&config.repository)?;
    let mut entries = Vec::new();
    for resource in ["releases", "tags"] {
        for page in 1..=10 {
            let response = http.get(
                &format!(
                    "https://api.github.com/repos/{}/{resource}?per_page=100&page={page}",
                    config.repository
                ),
                &[("X-GitHub-Api-Version", "2022-11-28")],
            )?;
            match response.status {
                200 => {},
                401 | 404 => return Ok(UpdateStatus::RepositoryUnavailable(
                    "暂时无法公开访问此仓库，请核对仓库名称。开发中的私有仓库会返回此状态；转为公开后即可直接检查更新，无需密钥。".into(),
                )),
                403 | 429 => return Err("GitHub 请求频率受限，请稍后重试".into()),
                other => return Err(format!("GitHub 返回 HTTP {other}，无法判断更新状态")),
            }
            let values = response.value.as_array().ok_or("GitHub 返回格式不正确")?;
            for value in values {
                if resource == "releases" {
                    entries.push(value.clone());
                } else if let Some(tag) = value["name"].as_str() {
                    // Prefer a published release over its matching tag.
                    if entries.iter().any(|r| r["tag_name"].as_str() == Some(tag)) {
                        continue;
                    }
                    let mut url =
                        url::Url::parse(&format!("https://github.com/{}/tree/", config.repository))
                            .map_err(|_| "仓库地址无效")?;
                    url.path_segments_mut()
                        .map_err(|_| "仓库地址无效")?
                        .pop_if_empty()
                        .push(tag);
                    entries.push(serde_json::json!({
                        "draft": false, "tag_name": tag, "html_url": url.as_str(), "tag_only": true,
                        "name": format!("Tag {tag}"),
                        "body": "此版本目前只有 Git Tag，尚未发布安装包。打开后可查看该版本源码。"
                    }));
                }
            }
            if values.len() < 100 {
                break;
            }
            if page == 10 {
                return Err("仓库版本数量超出检查范围，请打开 GitHub 查看完整版本列表".into());
            }
        }
    }
    select_release(&Value::Array(entries), config, current)
}
fn select_release(
    value: &Value,
    config: &UpdateConfig,
    current: &str,
) -> Result<UpdateStatus, String> {
    let releases = value.as_array().ok_or("GitHub 返回格式不正确")?;
    let current_version = Version::parse(current).map_err(|_| "当前版本无效")?;
    let mut candidates = Vec::new();
    for r in releases {
        if r["draft"].as_bool().unwrap_or(true) {
            continue;
        }
        let tag = r["tag_name"].as_str().unwrap_or("");
        let Ok(version) = Version::parse(tag.trim_start_matches('v')) else {
            continue;
        };
        let prerelease = r["prerelease"].as_bool().unwrap_or(false) || !version.pre.is_empty();
        if prerelease && !config.include_prerelease {
            continue;
        }
        let link = r["html_url"].as_str().unwrap_or("");
        let Ok(url) = url::Url::parse(link) else {
            continue;
        };
        let prefix = format!("/{}/releases/tag/", config.repository);
        if url.scheme() != "https"
            || url.host_str() != Some("github.com")
            || !(url.path().starts_with(&prefix)
                || url
                    .path()
                    .starts_with(&format!("/{}/tree/", config.repository)))
            || url.port().is_some()
            || !url.username().is_empty()
        {
            continue;
        }
        candidates.push((
            version,
            Release {
                version: tag.into(),
                name: r["name"].as_str().unwrap_or(tag).into(),
                url: link.into(),
                notes: r["body"]
                    .as_str()
                    .unwrap_or("")
                    .chars()
                    .take(16000)
                    .collect(),
                published_at: r["published_at"].as_str().unwrap_or("").into(),
                prerelease,
                tag_only: r["tag_only"].as_bool().unwrap_or(false),
            },
        ));
    }
    if candidates.is_empty() {
        return Ok(UpdateStatus::NoReleases);
    }
    candidates.sort_by(|a, b| b.0.cmp(&a.0));
    let (version, release) = candidates.remove(0);
    if version > current_version {
        Ok(UpdateStatus::Available(release))
    } else {
        Ok(UpdateStatus::UpToDate {
            current: current.into(),
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    struct TagOnly;
    impl JsonHttp for TagOnly {
        fn get(
            &self,
            url: &str,
            headers: &[(&str, &str)],
        ) -> Result<crate::http::JsonResponse, String> {
            assert!(
                !headers
                    .iter()
                    .any(|(name, _)| name.eq_ignore_ascii_case("authorization"))
            );
            Ok(crate::http::JsonResponse {
                status: 200,
                value: if url.contains("/tags?") {
                    json!([{"name":"v1.2.0"}, {"name":"v1.11.0"}])
                } else {
                    json!([])
                },
                etag: None,
            })
        }
    }
    #[test]
    fn public_tag_only_repository_finds_latest_without_credentials() {
        let result = check(&TagOnly, &UpdateConfig::default(), "1.0.0").unwrap();
        assert!(
            matches!(result, UpdateStatus::Available(ref r) if r.version == "v1.11.0"),
            "{result:?}"
        );
    }
    #[test]
    fn public_check_paginates_and_keeps_release_metadata() {
        struct Pages;
        impl JsonHttp for Pages {
            fn get(
                &self,
                url: &str,
                headers: &[(&str, &str)],
            ) -> Result<crate::http::JsonResponse, String> {
                assert!(
                    !headers
                        .iter()
                        .any(|(key, _)| key.eq_ignore_ascii_case("authorization"))
                );
                let value = if url.contains("/releases?") {
                    if url.ends_with("page=1") {
                        Value::Array(vec![release("v0.1.0", false); 100])
                    } else {
                        json!([{"draft":false,"tag_name":"v1.11.0","name":"Release notes","html_url":"https://github.com/Lianzy-Baimiao/LiteTray/releases/tag/v1.11.0"}])
                    }
                } else {
                    json!([{"name":"v1.11.0"},{"name":"v9.0.0-beta.1"}])
                };
                Ok(crate::http::JsonResponse {
                    status: 200,
                    value,
                    etag: None,
                })
            }
        }
        let UpdateStatus::Available(result) =
            check(&Pages, &UpdateConfig::default(), "1.0.0").unwrap()
        else {
            panic!()
        };
        assert_eq!(result.name, "Release notes");
        assert!(result.url.contains("/releases/tag/"));
    }
    #[test]
    fn inaccessible_repository_never_reports_up_to_date_or_requests_a_token() {
        struct Private;
        impl JsonHttp for Private {
            fn get(
                &self,
                _: &str,
                _: &[(&str, &str)],
            ) -> Result<crate::http::JsonResponse, String> {
                Ok(crate::http::JsonResponse {
                    status: 404,
                    value: Value::Null,
                    etag: None,
                })
            }
        }
        let status = check(&Private, &UpdateConfig::default(), "1.0.0").unwrap();
        assert!(matches!(status, UpdateStatus::RepositoryUnavailable(_)));
        assert!(!status.text().contains("Token"));
    }
    fn release(tag: &str, pre: bool) -> Value {
        json!({"draft":false,"tag_name":tag,"prerelease":pre,"html_url":format!("https://github.com/Lianzy-Baimiao/LiteTray/releases/tag/{tag}")})
    }
    #[test]
    fn sorts_semver_and_ignores_prerelease() {
        let c = UpdateConfig::default();
        let v = json!([
            release("v1.9.0", false),
            release("v1.10.0", false),
            release("v2.0.0-beta.1", true)
        ]);
        let UpdateStatus::Available(r) = select_release(&v, &c, "1.0.0").unwrap() else {
            panic!()
        };
        assert_eq!(r.version, "v1.10.0");
    }
    #[test]
    fn rejects_foreign_download_links() {
        let mut r = release("v9.0.0", false);
        r["html_url"] = json!("https://evil.example/releases/tag/v9.0.0");
        assert!(matches!(
            select_release(&json!([r]), &UpdateConfig::default(), "1.0.0").unwrap(),
            UpdateStatus::NoReleases
        ));
    }
    #[test]
    fn empty_repository_is_not_reported_up_to_date() {
        assert!(matches!(
            select_release(&json!([]), &UpdateConfig::default(), "1.0.0").unwrap(),
            UpdateStatus::NoReleases
        ));
    }
}
