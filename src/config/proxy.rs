use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProxyMode {
    #[default]
    System,
    Direct,
    Manual,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProxyProtocol {
    #[default]
    Http,
    Https,
    Socks5,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProxyConfig {
    pub mode: ProxyMode,
    pub protocol: ProxyProtocol,
    pub host: String,
    pub port: u16,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            mode: ProxyMode::System,
            protocol: ProxyProtocol::Http,
            host: "127.0.0.1".into(),
            port: 7890,
        }
    }
}

impl ProxyConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.mode == ProxyMode::Manual {
            if self.port == 0 {
                return Err("代理端口应为 1–65535 的整数".into());
            }
            self.canonical_host()?;
        }
        Ok(())
    }

    pub(crate) fn canonical_host(&self) -> Result<String, String> {
        let host = self.host.trim();
        if host.is_empty()
            || host.len() > 253
            || host.chars().any(|c| c.is_whitespace() || c.is_control())
            || host.contains(['/', '\\', '@', '?', '#'])
        {
            return Err("代理服务器应填写 IP 地址或域名".into());
        }
        if let Ok(address) = host.parse::<std::net::IpAddr>() {
            return Ok(match address {
                std::net::IpAddr::V4(ip) => ip.to_string(),
                std::net::IpAddr::V6(ip) => format!("[{ip}]"),
            });
        }
        url::Host::parse(host)
            .map(|host| host.to_string())
            .map_err(|_| "代理服务器应填写 IP 地址或域名".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_configs_follow_system_and_proxy_choices_roundtrip() {
        let legacy: crate::config::Config = serde_json::from_str("{}").unwrap();
        assert_eq!(legacy.proxy, ProxyConfig::default());
        for mode in [ProxyMode::System, ProxyMode::Direct, ProxyMode::Manual] {
            let proxy = ProxyConfig {
                mode,
                protocol: ProxyProtocol::Socks5,
                host: "127.0.0.1".into(),
                port: 1080,
            };
            proxy.validate().unwrap();
            let restored: ProxyConfig =
                serde_json::from_str(&serde_json::to_string(&proxy).unwrap()).unwrap();
            assert_eq!(restored, proxy);
        }
    }

    #[test]
    fn manual_proxy_rejects_invalid_addresses_and_accepts_ipv6() {
        for host in [
            "",
            "http://localhost",
            "a@localhost",
            "a b",
            "a/path",
            "host:7890",
            "host\nname",
        ] {
            let proxy = ProxyConfig {
                mode: ProxyMode::Manual,
                host: host.into(),
                ..Default::default()
            };
            assert!(proxy.validate().is_err(), "accepted invalid host: {host:?}");
        }
        let mut proxy = ProxyConfig {
            mode: ProxyMode::Manual,
            ..Default::default()
        };
        proxy.port = 0;
        assert!(proxy.validate().is_err());
        proxy.port = 65535;
        for (host, expected) in [
            ("::1", "[::1]"),
            ("[::1]", "[::1]"),
            ("127.0.0.1", "127.0.0.1"),
            ("PROXY.example", "proxy.example"),
        ] {
            proxy.host = host.into();
            proxy.validate().unwrap();
            assert_eq!(proxy.canonical_host().unwrap(), expected);
        }
        proxy.mode = ProxyMode::Direct;
        proxy.host = "".into();
        proxy.port = 0;
        proxy.validate().unwrap();
    }
}
