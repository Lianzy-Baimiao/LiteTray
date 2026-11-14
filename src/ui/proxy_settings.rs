use super::*;
use config::{ProxyConfig, ProxyMode, ProxyProtocol};
use http::JsonHttp;

pub(super) const PAGE: usize = 12;
pub(super) const MODE: u16 = 660;
pub(super) const PROTOCOL: u16 = 661;
pub(super) const HOST: u16 = 662;
pub(super) const PORT: u16 = 663;
pub(super) const TEST: u16 = 664;
const MODE_NOTE: u16 = 665;
const TEST_RESULT: u16 = 666;

#[derive(Default)]
pub(super) struct State {
    serial: u64,
    pub active: Option<(u64, ProxyConfig)>,
    invalidated: bool,
    message: String,
}

impl App {
    pub(super) unsafe fn proxy_settings(&mut self, c: &Config, width: i32) {
        unsafe {
            let total = width - 260;
            self.section("连接方式", 224, 88, total);
            self.combo(
                &["跟随系统代理", "直接连接", "手动代理"],
                match c.proxy.mode {
                    ProxyMode::System => 0,
                    ProxyMode::Direct => 1,
                    ProxyMode::Manual => 2,
                },
                MODE,
                224,
                126,
                total,
            );
            self.control("STATIC", "", MODE_NOTE, SS_LEFT as u32, 224, 174, total, 48);
            self.divider(224, 238, total);
            self.section("手动代理", 224, 254, total);
            self.label("代理协议", 224, 294, 150, 22);
            self.combo(
                &["HTTP", "HTTPS", "SOCKS5"],
                match c.proxy.protocol {
                    ProxyProtocol::Http => 0,
                    ProxyProtocol::Https => 1,
                    ProxyProtocol::Socks5 => 2,
                },
                PROTOCOL,
                224,
                319,
                150,
            );
            self.edit("服务器地址", &c.proxy.host, HOST, 390, 294, total - 322);
            self.edit(
                "代理端口",
                &c.proxy.port.to_string(),
                PORT,
                224 + total - 140,
                294,
                140,
            );
            self.caption(
                "填写 IP 或域名与代理软件提供的端口，例如 127.0.0.1:7890。\r\n多数本机代理选择 HTTP；HTTPS 要求代理服务器支持加密连接。",
                224,
                366,
                total,
                48,
            );
            self.divider(224, 418, total);
            self.section("连接测试", 224, 434, total - 196);
            self.button("测试连接", TEST, 224 + total - 172, 429, 172);
            self.control(
                "STATIC",
                "",
                TEST_RESULT,
                SS_LEFT as u32,
                224,
                477,
                total,
                56,
            );
            self.caption("保存后用于天气、城市搜索、节假日更新、版本检查与符号下载。\r\n测试连接仅检查当前选择，不保存设置。", 224, 539, total, 48);
            self.sync_proxy_controls();
        }
    }

    pub(super) fn collect_proxy(&self) -> Result<ProxyConfig, String> {
        let mode = match self.selected(MODE) {
            1 => ProxyMode::Direct,
            2 => ProxyMode::Manual,
            _ => ProxyMode::System,
        };
        let port = match self.text(PORT).trim().parse::<u16>() {
            Ok(port) => port,
            Err(_) if mode != ProxyMode::Manual => self.draft.proxy.port,
            Err(_) => return Err(tr("代理端口应为 1–65535 的整数").into()),
        };
        Ok(ProxyConfig {
            mode,
            protocol: match self.selected(PROTOCOL) {
                1 => ProxyProtocol::Https,
                2 => ProxyProtocol::Socks5,
                _ => ProxyProtocol::Http,
            },
            host: self.text(HOST).trim().into(),
            port,
        })
    }

    pub(super) unsafe fn sync_proxy_controls(&self) {
        unsafe {
            if self.page != PAGE {
                return;
            }
            let mode = self.selected(MODE);
            for id in [PROTOCOL, HOST, PORT] {
                EnableWindow(self.field(id), i32::from(mode == 2));
            }
            EnableWindow(
                self.field(TEST),
                i32::from(self.proxy_test.active.is_none()),
            );
            set_text(
                self.field(TEST),
                if self.proxy_test.active.is_some() {
                    "正在测试…"
                } else {
                    "测试连接"
                },
            );
            set_text(
                self.field(MODE_NOTE),
                match mode {
                    1 => "不使用代理，直接访问网络。",
                    2 => "使用下方的手动代理。SOCKS5 由代理服务器解析域名。",
                    _ => "使用 Windows 系统代理；代理环境变量优先。",
                },
            );
            set_text(
                self.field(TEST_RESULT),
                if self.proxy_test.message.is_empty() {
                    "点击“测试连接”检查 Open-Meteo 天气服务是否可访问。"
                } else {
                    &self.proxy_test.message
                },
            );
        }
    }

    pub(super) unsafe fn proxy_input_changed(&mut self) {
        unsafe {
            self.proxy_test.invalidated |= self.proxy_test.active.is_some();
            self.proxy_test.message = if self.proxy_test.active.is_some() {
                tr("代理设置已修改；当前测试结果将被忽略，请完成后重新测试。").into()
            } else {
                String::new()
            };
            self.sync_proxy_controls();
        }
    }

    pub(super) unsafe fn start_proxy_test(&mut self) {
        unsafe {
            if self.proxy_test.active.is_some() {
                return;
            }
            let proxy = match self.collect_proxy().and_then(|proxy| {
                proxy.validate().map_err(|error| tr(&error).to_string())?;
                Ok(proxy)
            }) {
                Ok(proxy) => proxy,
                Err(error) => {
                    self.proxy_test.message = error;
                    self.sync_proxy_controls();
                    return;
                }
            };
            let client = match http::Client::with_proxy(8, &proxy) {
                Ok(client) => client,
                Err(error) => {
                    self.proxy_test.message = tr(&error).into();
                    self.sync_proxy_controls();
                    return;
                }
            };
            self.proxy_test.serial = self.proxy_test.serial.wrapping_add(1);
            let serial = self.proxy_test.serial;
            self.proxy_test.active = Some((serial, proxy.clone()));
            self.proxy_test.invalidated = false;
            self.proxy_test.message = tr("正在检查 Open-Meteo 连接…").into();
            self.sync_proxy_controls();
            let weather = self.saved.weather.clone();
            let sender = self.tx.clone();
            let window = self.hwnd as usize;
            thread::spawn(move || {
                let started = Instant::now();
                let result = probe(&client, &weather).map(|()| started.elapsed());
                let _ = sender.send(Event::ProxyTest(serial, proxy, result));
                PostMessageW(window as HWND, WM_RESULT, 0, 0);
            });
        }
    }

    pub(super) unsafe fn receive_proxy_test(
        &mut self,
        serial: u64,
        proxy: ProxyConfig,
        result: Result<Duration, String>,
    ) {
        unsafe {
            if self
                .proxy_test
                .active
                .as_ref()
                .is_none_or(|(active, _)| *active != serial)
            {
                return;
            }
            self.proxy_test.active = None;
            let current = if self.page == PAGE {
                self.collect_proxy().ok()
            } else {
                Some(self.draft.proxy.clone())
            };
            self.proxy_test.message = if self.proxy_test.invalidated
                || current.as_ref() != Some(&proxy)
            {
                tr("代理设置已修改，请重新测试。").into()
            } else {
                match result {
                    Ok(elapsed) if i18n::english() => format!(
                        "Connection succeeded · Open-Meteo · {:.2} s",
                        elapsed.as_secs_f64()
                    ),
                    Ok(elapsed) => {
                        format!("连接成功 · Open-Meteo · {:.2} 秒", elapsed.as_secs_f64())
                    }
                    Err(error) if i18n::english() => format!("Connection failed: {}", tr(&error)),
                    Err(error) => format!("连接失败：{error}"),
                }
            };
            self.sync_proxy_controls();
        }
    }
}

fn probe(client: &dyn JsonHttp, weather: &config::WeatherConfig) -> Result<(), String> {
    let mut url = url::Url::parse("https://api.open-meteo.com/v1/forecast").unwrap();
    url.query_pairs_mut()
        .append_pair("latitude", &weather.latitude.to_string())
        .append_pair("longitude", &weather.longitude.to_string())
        .append_pair("current", "temperature_2m")
        .append_pair("forecast_days", "1");
    let response = client.get(url.as_str(), &[])?;
    if response.status != 200 {
        return Err(format!("Open-Meteo HTTP {}", response.status));
    }
    if response.value["current"]["temperature_2m"]
        .as_f64()
        .is_none_or(|value| !value.is_finite())
    {
        return Err("天气服务返回的数据无效".into());
    }
    Ok(())
}
