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
                .map_err(|error| {
                    format!(
                        "读取 Microsoft 符号文件失败（已读取 {} 字节）：{error}",
                        bytes.len()
                    )
                })?;
            if bytes.len() > 64 * 1024 * 1024 || !bytes.starts_with(b"Microsoft C/C++ MSF 7.00") {
                return Err("符号文件格式或大小无效".into());
            }
            return Ok(bytes);
        }
        Err("Microsoft 符号服务器重定向过多".into())
    }
    pub fn new(timeout_seconds: u64) -> Self {
        Self::build(timeout_seconds, ureq::Proxy::try_from_env())
    }

    pub fn with_proxy(
        timeout_seconds: u64,
        config: &crate::config::ProxyConfig,
    ) -> Result<Self, String> {
        use crate::config::{ProxyMode, ProxyProtocol};
        config.validate()?;
        let proxy = match config.mode {
            ProxyMode::System => ureq::Proxy::try_from_env(),
            ProxyMode::Direct => None,
            ProxyMode::Manual => {
                let protocol = match config.protocol {
                    ProxyProtocol::Http => ureq::ProxyProtocol::Http,
                    ProxyProtocol::Https => ureq::ProxyProtocol::Https,
                    ProxyProtocol::Socks5 => ureq::ProxyProtocol::Socks5,
                };
                Some(
                    ureq::Proxy::builder(protocol)
                        .host(&config.canonical_host()?)
                        .port(config.port)
                        .resolve_target(false)
                        .build()
                        .map_err(|_| "代理地址无效，请检查服务器与端口".to_string())?,
                )
            }
        };
        Ok(Self::build(timeout_seconds, proxy))
    }

    pub fn from_saved_settings(timeout_seconds: u64) -> Result<Self, String> {
        let config = crate::config::load(&crate::config::data_dir().join("config.json"))?;
        Self::with_proxy(timeout_seconds, &config.proxy)
    }

    fn build(timeout_seconds: u64, proxy: Option<ureq::Proxy>) -> Self {
        use ureq::tls::{RootCerts, TlsConfig, TlsProvider};
        let config = ureq::Agent::config_builder()
            .proxy(proxy)
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

#[cfg(all(test, windows))]
#[path = "../tests/support/windows_registry.rs"]
mod registry_test_support;

#[cfg(all(test, windows))]
#[path = "http/proxy_checks.rs"]
mod proxy_checks;

#[cfg(all(test, windows))]
mod tests {
    use super::registry_test_support as registry;
    use super::*;

    #[test]
    fn windows_system_proxy_carries_the_actual_https_request() {
        registry::in_child(|| {
            use std::{
                io::{BufRead, BufReader, Write},
                net::TcpListener,
                thread,
                time::Instant,
            };
            let registry = registry::CurrentUser::new();
            for name in ["ALL_PROXY", "HTTPS_PROXY", "HTTP_PROXY", "NO_PROXY"] {
                // This test is already in its dedicated child process.
                unsafe {
                    std::env::remove_var(name);
                }
            }
            let proxy = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = proxy.local_addr().unwrap().to_string();
            let settings = "Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings";
            registry.write(
                settings,
                "ProxyEnable",
                windows_sys::Win32::System::Registry::REG_DWORD,
                &1_u32.to_le_bytes(),
            );
            registry.write_string(settings, "ProxyServer", &address);
            assert_eq!(registry.string(settings, "ProxyServer"), Some(address));

            proxy.set_nonblocking(true).unwrap();
            let server = thread::spawn(move || {
                let deadline = Instant::now() + Duration::from_secs(2);
                while Instant::now() < deadline {
                    if let Ok((mut stream, _)) = proxy.accept() {
                        stream.set_nonblocking(false).unwrap();
                        stream
                            .set_read_timeout(Some(Duration::from_secs(1)))
                            .unwrap();
                        let mut request = String::new();
                        BufReader::new(&mut stream).read_line(&mut request).unwrap();
                        stream.write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
                        return request;
                    }
                    thread::sleep(Duration::from_millis(5));
                }
                String::new()
            });
            let result = Client::new(1).get("https://litetray-weather-probe.invalid/current", &[]);
            let request = server.join().unwrap();
            assert!(result.is_err());
            assert!(
                request.starts_with("CONNECT litetray-weather-probe.invalid:443 "),
                "the HTTPS request bypassed the enabled Windows proxy: {request:?}"
            );
        });
    }
}
