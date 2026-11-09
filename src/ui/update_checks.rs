use super::*;

#[test]
fn release_matching_the_settings_version_is_already_up_to_date() {
    struct CurrentRelease;
    impl http::JsonHttp for CurrentRelease {
        fn get(&self, url: &str, _: &[(&str, &str)]) -> Result<http::JsonResponse, String> {
            let displayed_version = UI_REVISION.strip_prefix("开发版 ").unwrap();
            let tag = format!("v{displayed_version}");
            Ok(http::JsonResponse {
                status: 200,
                value: if url.contains("/releases?") {
                    serde_json::json!([{
                        "draft": false,
                        "tag_name": tag,
                        "html_url": format!("https://github.com/Lianzy-Baimiao/LiteTray/releases/tag/{tag}")
                    }])
                } else {
                    serde_json::json!([{"name": tag}])
                },
                etag: None,
            })
        }
    }
    let result = update::check_current(&CurrentRelease, &config::UpdateConfig::default()).unwrap();
    assert!(
        matches!(&result, UpdateStatus::UpToDate { current } if UI_REVISION.ends_with(current)),
        "Settings displays {UI_REVISION}, but checking that same release reports: {}",
        result.text()
    );
}
