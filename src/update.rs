use crate::{
    config::{UpdateConfig, validate_repository},
    http::JsonHttp,
};
use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const CREDENTIAL_TARGET: &str = "LiteTray/github/releases";
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Release {
    pub version: String,
    pub name: String,
    pub url: String,
    pub notes: String,
    pub published_at: String,
    pub prerelease: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UpdateStatus {
    Available(Release),
    UpToDate { current: String },
    NoReleases,
    AccessRequired(String),
}
impl UpdateStatus {
    pub fn text(&self) -> String {
        match self {
            Self::Available(r) => format!(
                "发现新版本 {}{}",
                r.version,
                if r.prerelease { "（预览版）" } else { "" }
            ),
            Self::UpToDate { current } => format!("已是最新版本 {current}"),
            Self::NoReleases => "仓库已连接，尚未发布可用版本".into(),
            Self::AccessRequired(s) => s.clone(),
        }
    }
}
pub fn check(
    http: &dyn JsonHttp,
    config: &UpdateConfig,
    token: Option<&str>,
    current: &str,
) -> Result<UpdateStatus, String> {
    validate_repository(&config.repository)?;
    let mut headers = vec![("X-GitHub-Api-Version", "2022-11-28")];
    let bearer = token
        .filter(|t| !t.is_empty())
        .map(|t| format!("Bearer {t}"));
    if let Some(v) = &bearer {
        headers.push(("Authorization", v));
    }
    let response = http.get(
        &format!(
            "https://api.github.com/repos/{}/releases?per_page=100",
            config.repository
        ),
        &headers,
    )?;
    match response.status {
        200 => select_release(&response.value, config, current),
        401 => Ok(UpdateStatus::AccessRequired(
            "GitHub 凭据无效或已过期，请重新保存 Token".into(),
        )),
        404 => Ok(UpdateStatus::AccessRequired(
            "仓库不存在或为私有仓库；请核对仓库并保存具有此仓库 Contents 读取权限的 Token".into(),
        )),
        403 | 429 => Err("GitHub 请求被限制或凭据权限不足，请稍后重试并检查权限".into()),
        other => Err(format!("GitHub 返回 HTTP {other}，无法判断更新状态")),
    }
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
            || !url.path().starts_with(&prefix)
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
