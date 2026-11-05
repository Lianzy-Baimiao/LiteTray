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
    pub fn download_symbols(&self, url: &str) -> Result<Vec<u8>, String> {
        use std::io::Read;
        let mut address = url::Url::parse(url).map_err(|_| "符号地址无效")?;
        for _ in 0..4 {
            let host = address.host_str().ok_or("符号地址缺少主机")?;
            if address.scheme() != "https"
                || !(host == "msdl.microsoft.com"
                    || host == "download.microsoft.com"
                    || host.ends_with(".blob.core.windows.net"))
            {
                return Err("符号下载重定向到非 Microsoft HTTPS 地址".into());
            }
            let mut response = self
                .agent
                .get(address.as_str())
                .call()
                .map_err(|_| "Microsoft 符号下载失败或超时")?;
            if response.status().is_redirection() {
                let location = response
                    .headers()
                    .get("location")
                    .and_then(|v| v.to_str().ok())
                    .ok_or("符号重定向缺少地址")?;
                address = address.join(location).map_err(|_| "符号重定向地址无效")?;
                continue;
            }
            if response.status().as_u16() != 200 {
                return Err(format!(
                    "Microsoft 符号服务器返回 HTTP {}",
                    response.status().as_u16()
                ));
            }
            let mut bytes = Vec::new();
            response
                .body_mut()
                .as_reader()
                .take(64 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| "读取符号文件失败")?;
            if bytes.len() > 64 * 1024 * 1024 || !bytes.starts_with(b"Microsoft C/C++ MSF 7.00") {
                return Err("符号文件格式或大小无效".into());
            }
            return Ok(bytes);
        }
        Err("Microsoft 符号服务器重定向过多".into())
    }
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
