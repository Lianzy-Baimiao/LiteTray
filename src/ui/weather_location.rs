//! Location requests run independently of page lifetime. Changing the query
//! or adopting another place makes an outstanding reply obsolete.
use super::*;
pub(super) const QUERY: u16 = 401;
pub(super) const SEARCH: u16 = 417;
pub(super) const NEARBY: u16 = 418;
pub(super) const CHOICE: u16 = 419;
const MESSAGE: u16 = 420;
type Position = (String, f64, f64);
fn position(c: &config::WeatherConfig) -> Position {
    (c.city.clone(), c.latitude, c.longitude)
}
#[derive(Clone)]
pub(super) struct Pending {
    serial: u64,
    nearby: bool,
    query: String,
    position: Position,
}
#[derive(Default)]
pub(super) struct State {
    pub query: String,
    pub results: Vec<location::Place>,
    pub message: String,
    pub active: Option<Pending>,
    serial: u64,
}
impl State {
    fn begin(&mut self, nearby: bool, weather: &config::WeatherConfig) -> Result<Pending, String> {
        if self.active.is_some() {
            return Err("正在查询位置，请稍候".into());
        }
        if !nearby {
            location::validate_query(&self.query)?;
        }
        self.serial = self.serial.wrapping_add(1);
        let pending = Pending {
            serial: self.serial,
            nearby,
            query: self.query.clone(),
            position: position(weather),
        };
        self.active = Some(pending.clone());
        self.message = if nearby {
            "正在获取当地位置…"
        } else {
            "正在搜索城市…"
        }
        .into();
        Ok(pending)
    }
}
impl App {
    pub(super) unsafe fn location_controls(&mut self, total: i32) {
        unsafe {
            self.control(
                "EDIT",
                &self.location.query.clone(),
                QUERY,
                WS_TABSTOP | ES_AUTOHSCROLL as u32,
                224,
                128,
                total - 236,
                30,
            );
            SendMessageW(self.field(QUERY), EM_SETLIMITTEXT, 80, 0);
            SendMessageW(
                self.field(QUERY),
                EM_SETCUEBANNER,
                0,
                wide(tr("输入城市名、区县名或拼音")).as_ptr() as isize,
            );
            SetWindowSubclass(self.field(QUERY), Some(search_edit_proc), 23, 0);
            self.button("搜索城市", SEARCH, 224 + total - 220, 126, 100);
            self.button("获取当地", NEARBY, 224 + total - 108, 126, 108);
            self.combo(&[""], 0, CHOICE, 224, 173, total);
            SendMessageW(self.field(CHOICE), CB_SETMINVISIBLE, 8, 0);
            let message = self.control("STATIC", "", MESSAGE, SS_LEFT as u32, 224, 213, total, 44);
            SetPropW(message, wide("LiteTray.Muted").as_ptr(), 1usize as HANDLE);
            self.sync_location_controls();
        }
    }
    pub(super) unsafe fn sync_location_controls(&self) {
        unsafe {
            if self.page != 3 {
                return;
            }
            for id in [SEARCH, NEARBY] {
                EnableWindow(self.field(id), self.location.active.is_none() as i32);
            }
            let combo = self.field(CHOICE);
            SendMessageW(combo, WM_SETREDRAW, 0, 0);
            SendMessageW(combo, CB_RESETCONTENT, 0, 0);
            SendMessageW(
                combo,
                CB_ADDSTRING,
                0,
                wide(&format!("当前位置：{}", self.draft.weather.city)).as_ptr() as isize,
            );
            let mut selected = 0;
            for (index, place) in self.location.results.iter().enumerate() {
                SendMessageW(
                    combo,
                    CB_ADDSTRING,
                    0,
                    wide(&place.label()).as_ptr() as isize,
                );
                if self.draft.weather.latitude == place.latitude
                    && self.draft.weather.longitude == place.longitude
                    && self.draft.weather.city == place.name
                {
                    selected = index + 1;
                }
            }
            SendMessageW(combo, CB_SETCURSEL, selected, 0);
            SendMessageW(combo, WM_SETREDRAW, 1, 0);
            EnableWindow(combo, i32::from(!self.location.results.is_empty()));
            InvalidateRect(combo, null(), 0);
            set_text(
                self.field(MESSAGE),
                if self.location.message.is_empty() {
                    "搜索由 Open-Meteo / GeoNames 提供，无需填写经纬度。\r\n获取当地按公网 IP 定位，可能受代理或 VPN 影响。"
                } else {
                    &self.location.message
                },
            );
        }
    }
    pub(super) unsafe fn start_location(&mut self, nearby: bool) {
        unsafe {
            self.location.query = self.text(QUERY);
            let pending = match self.location.begin(nearby, &self.draft.weather) {
                Ok(pending) => pending,
                Err(error) => {
                    self.location.message = error;
                    self.sync_location_controls();
                    return;
                }
            };
            self.sync_location_controls();
            let sender = self.tx.clone();
            let window = self.hwnd as usize;
            thread::spawn(move || {
                let client = http::Client::new(8);
                let result = if nearby {
                    location::nearby(&client).map(|place| vec![place])
                } else {
                    location::search(&client, &pending.query)
                };
                let _ = sender.send(Event::Location(pending.serial, result));
                PostMessageW(window as HWND, WM_RESULT, 0, 0);
            });
        }
    }
    pub(super) unsafe fn receive_location(
        &mut self,
        serial: u64,
        result: Result<Vec<location::Place>, String>,
    ) {
        unsafe {
            if self
                .location
                .active
                .as_ref()
                .is_none_or(|p| p.serial != serial)
            {
                return;
            }
            let pending = self.location.active.take().unwrap();
            if pending.query != self.location.query
                || pending.position != position(&self.draft.weather)
            {
                self.location.message = "位置或搜索内容已修改，已忽略旧查询；可重新搜索。".into();
                self.sync_location_controls();
                return;
            }
            match result {
                Ok(places) => {
                    self.location.results = places;
                    if self.location.results.is_empty() {
                        self.location.message =
                            "没有找到匹配城市，试试城市名或拼音；原位置已保留。".into();
                    } else if pending.nearby {
                        self.adopt_location(0);
                    } else {
                        self.location.message = format!(
                            "找到 {} 个结果，请展开上方列表选择省市相符的位置。\r\nOpen-Meteo / GeoNames · 选择后点击“保存设置”生效。",
                            self.location.results.len()
                        );
                    }
                }
                Err(error) => {
                    self.location.message = error;
                }
            }
            self.sync_location_controls();
        }
    }
    pub(super) unsafe fn choose_location(&mut self) {
        unsafe {
            let index = SendMessageW(self.field(CHOICE), CB_GETCURSEL, 0, 0);
            if index > 0 {
                self.adopt_location(index as usize - 1);
            }
            self.sync_location_controls();
        }
    }
    unsafe fn adopt_location(&mut self, index: usize) {
        if let Some(place) = self.location.results.get(index) {
            self.draft.weather.city = place.name.clone();
            self.draft.weather.latitude = place.latitude;
            self.draft.weather.longitude = place.longitude;
            self.location.message = if place.source == "Open-Meteo / GeoNames" {
                "位置已更新，经纬度已自动填写。\r\nOpen-Meteo / GeoNames · 点击“保存设置”生效。"
                    .into()
            } else {
                format!(
                    "{} 获取到近似位置，点击“保存设置”生效。\r\nIP 定位可能受代理影响；不准确时请搜索城市校正。",
                    place.source
                )
            };
        }
    }
}
unsafe extern "system" fn search_edit_proc(
    hwnd: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
    id: usize,
    _: usize,
) -> LRESULT {
    unsafe {
        if msg == WM_GETDLGCODE && l != 0 && (*(l as *const MSG)).wParam == VK_RETURN as usize {
            return DefSubclassProc(hwnd, msg, w, l) | DLGC_WANTMESSAGE as isize;
        }
        if msg == WM_KEYDOWN && w == VK_RETURN as usize {
            PostMessageW(GetAncestor(hwnd, GA_ROOT), WM_COMMAND, SEARCH as usize, 0);
            return 0;
        }
        if msg == WM_CHAR && w == VK_RETURN as usize {
            return 0;
        }
        if msg == WM_NCDESTROY {
            RemoveWindowSubclass(hwnd, Some(search_edit_proc), id);
        }
        DefSubclassProc(hwnd, msg, w, l)
    }
}

#[cfg(test)]
pub(super) unsafe fn begin_for_check(app: &mut App, nearby: bool) -> u64 {
    app.location
        .begin(nearby, &app.draft.weather)
        .unwrap()
        .serial
}
