use super::{Client, JsonHttp, registry_test_support as registry};
use crate::config::{ProxyConfig, ProxyMode, ProxyProtocol};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    thread,
    time::{Duration, Instant},
};

fn system_proxy(registry: &registry::CurrentUser, address: &str) {
    let settings = "Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings";
    registry.write(
        settings,
        "ProxyEnable",
        windows_sys::Win32::System::Registry::REG_DWORD,
        &1_u32.to_le_bytes(),
    );
    registry.write_string(settings, "ProxyServer", address);
}

fn accept(listener: TcpListener) -> TcpStream {
    listener.set_nonblocking(true).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if let Ok((stream, _)) = listener.accept() {
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(1)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(1)))
                .unwrap();
            return stream;
        }
        thread::sleep(Duration::from_millis(5));
    }
    panic!("request did not reach the selected connection route");
}

#[test]
fn direct_mode_bypasses_both_windows_and_environment_proxies() {
    registry::in_child(|| {
        let registry = registry::CurrentUser::new();
        let trap = TcpListener::bind("127.0.0.1:0").unwrap();
        system_proxy(&registry, &trap.local_addr().unwrap().to_string());
        unsafe {
            std::env::set_var(
                "ALL_PROXY",
                format!("http://{}", trap.local_addr().unwrap()),
            );
            std::env::remove_var("NO_PROXY");
        }
        for environment_proxy in [true, false] {
            if !environment_proxy {
                for name in ["ALL_PROXY", "HTTPS_PROXY", "HTTP_PROXY"] {
                    unsafe {
                        std::env::remove_var(name);
                    }
                }
            }
            let target = TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("https://{}/current", target.local_addr().unwrap());
            let server = thread::spawn(move || {
                let mut stream = accept(target);
                let mut header = [0; 3];
                stream.read_exact(&mut header).unwrap();
                header
            });
            let client = Client::with_proxy(
                1,
                &ProxyConfig {
                    mode: ProxyMode::Direct,
                    ..Default::default()
                },
            )
            .unwrap();
            assert!(client.agent.config().proxy().is_none());
            assert!(client.get(&url, &[]).is_err()); // The fake endpoint stops at TLS ClientHello.
            let header = server.join().unwrap();
            assert_eq!(
                &header[..2],
                &[0x16, 0x03],
                "a direct HTTPS request must reach the target as TLS"
            );
        }
        trap.set_nonblocking(true).unwrap();
        assert!(
            trap.accept().is_err(),
            "direct mode contacted a disabled proxy"
        );
    });
}

#[test]
fn manual_http_overrides_system_environment_and_bypass_rules() {
    registry::in_child(|| {
        let registry = registry::CurrentUser::new();
        let trap = TcpListener::bind("127.0.0.1:0").unwrap();
        system_proxy(&registry, &trap.local_addr().unwrap().to_string());
        unsafe {
            std::env::set_var(
                "ALL_PROXY",
                format!("http://{}", trap.local_addr().unwrap()),
            );
            std::env::set_var("NO_PROXY", "*");
        }
        let selected = TcpListener::bind("127.0.0.1:0").unwrap();
        let config = ProxyConfig {
            mode: ProxyMode::Manual,
            port: selected.local_addr().unwrap().port(),
            ..Default::default()
        };
        let server = thread::spawn(move || {
            let mut stream = accept(selected);
            let mut request = String::new();
            BufReader::new(&mut stream).read_line(&mut request).unwrap();
            stream
                .write_all(
                    b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .unwrap();
            request
        });
        let client = Client::with_proxy(1, &config).unwrap();
        assert!(
            client
                .get("https://manual-proxy-probe.invalid/current", &[])
                .is_err()
        );
        assert!(
            server
                .join()
                .unwrap()
                .starts_with("CONNECT manual-proxy-probe.invalid:443 ")
        );
        trap.set_nonblocking(true).unwrap();
        assert!(
            trap.accept().is_err(),
            "manual mode must override automatic proxy discovery"
        );
        let tls_proxy = TcpListener::bind("127.0.0.1:0").unwrap();
        let tls_port = tls_proxy.local_addr().unwrap().port();
        let tls_server = thread::spawn(move || {
            let mut stream = accept(tls_proxy);
            let mut header = [0; 3];
            stream.read_exact(&mut header).unwrap();
            header
        });
        let https = Client::with_proxy(
            1,
            &ProxyConfig {
                protocol: ProxyProtocol::Https,
                port: tls_port,
                ..config
            },
        )
        .unwrap();
        assert_eq!(
            https.agent.config().proxy().unwrap().protocol(),
            ureq::ProxyProtocol::Https
        );
        assert!(
            https
                .get("https://manual-proxy-probe.invalid/current", &[])
                .is_err()
        );
        assert_eq!(
            &tls_server.join().unwrap()[..2],
            &[0x16, 0x03],
            "an HTTPS proxy must receive TLS before CONNECT"
        );
    });
}

#[test]
fn socks5_resolves_target_hostname_through_the_manual_proxy() {
    registry::in_child(|| {
        let selected = TcpListener::bind("127.0.0.1:0").unwrap();
        let config = ProxyConfig {
            mode: ProxyMode::Manual,
            protocol: ProxyProtocol::Socks5,
            port: selected.local_addr().unwrap().port(),
            ..Default::default()
        };
        let server = thread::spawn(move || {
            let mut stream = accept(selected);
            let mut greeting = [0; 2];
            stream.read_exact(&mut greeting).unwrap();
            assert_eq!(greeting[0], 5);
            let mut methods = vec![0; greeting[1] as usize];
            stream.read_exact(&mut methods).unwrap();
            assert!(methods.contains(&0));
            stream.write_all(&[5, 0]).unwrap();
            let mut request = [0; 5];
            stream.read_exact(&mut request).unwrap();
            assert_eq!(
                &request[..4],
                &[5, 1, 0, 3],
                "SOCKS5 must send a domain rather than resolve DNS locally"
            );
            let mut host = vec![0; request[4] as usize];
            stream.read_exact(&mut host).unwrap();
            let mut port = [0; 2];
            stream.read_exact(&mut port).unwrap();
            assert_eq!(u16::from_be_bytes(port), 443);
            stream.write_all(&[5, 4, 0, 1, 0, 0, 0, 0, 0, 0]).unwrap();
            String::from_utf8(host).unwrap()
        });
        assert!(
            Client::with_proxy(1, &config)
                .unwrap()
                .get("https://socks-proxy-probe.invalid/current", &[])
                .is_err()
        );
        assert_eq!(server.join().unwrap(), "socks-proxy-probe.invalid");
    });
}
