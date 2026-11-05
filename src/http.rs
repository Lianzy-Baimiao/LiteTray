use serde_json::Value;
use std::time::Duration;

pub struct JsonResponse {
    pub status: u16,
    pub value: Value,
    pub etag: Option<String>,
}
pub trait JsonHttp {
    fn get(&self, url: &str, headers: &[(&str, &str)]) -> Result<JsonResponse, String>;
}
pub struct Client {
    agent: ureq::Agent,
}
impl Client {
    pub fn new(timeout_seconds: u64) -> Self {
        use ureq::tls::{RootCerts, TlsConfig, TlsProvider};
        let config = ureq::Agent::config_builder()
            .https_only(true)
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(timeout_seconds)))
            .max_redirects(0)
            .user_agent(concat!(
                "LiteTray/",
                env!("CARGO_PKG_VERSION"),
                " (https://github.com/Lianzy-Baimiao)"
            ))
            .tls_config(
                TlsConfig::builder()
                    .provider(TlsProvider::NativeTls)
                    .root_certs(RootCerts::PlatformVerifier)
                    .build(),
            )
            .build();
        Self {
            agent: config.into(),
        }
    }
}
impl JsonHttp for Client {
    fn get(&self, url: &str, headers: &[(&str, &str)]) -> Result<JsonResponse, String> {
        let parsed = url::Url::parse(url).map_err(|_| "请求地址无效")?;
        if parsed.scheme() != "https" || parsed.host_str().is_none() {
            return Err("仅允许 HTTPS 数据源".into());
        }
        let mut request = self.agent.get(url).header("Accept", "application/json");
        for (key, value) in headers {
            request = request.header(*key, *value);
        }
        // Do not expose the request URL, which can contain a provider API key.
        let mut response = request
            .call()
            .map_err(|_| "网络请求失败或超时，请检查网络与代理设置".to_string())?;
        let status = response.status().as_u16();
        let etag = response
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        if status == 304 {
            return Ok(JsonResponse {
                status,
                value: Value::Null,
                etag,
            });
        }
        if status != 200 {
            return Ok(JsonResponse {
                status,
                value: Value::Null,
                etag,
            });
        }
        let value = response
            .body_mut()
            .with_config()
            .limit(2 * 1024 * 1024)
            .read_json()
            .map_err(|_| "服务返回的数据无效或超过 2 MB".to_string())?;
        Ok(JsonResponse {
            status,
            value,
            etag,
        })
    }
}
