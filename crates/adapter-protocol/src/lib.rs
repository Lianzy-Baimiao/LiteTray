//! Versioned, bounded data exchanged with the taskbar adapter. This crate has
//! no Windows/process APIs and can validate a request before any native call.
use serde::{Deserialize, Serialize};
pub mod activation;
pub mod clock;
pub mod routing;
pub mod topology;
pub const VERSION: u32 = 1;
pub const REQUEST_TAG: usize = 0x4c544101;
pub const RESPONSE_TAG: usize = 0x4c544102;
pub const RECEIVER_CLASS: &str = "LiteTray.AdapterReceiver.v1";
pub const MAX_REQUEST: usize = 32 * 1024;
pub const MAX_RESPONSE: usize = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleIdentity {
    pub machine: u16,
    pub timestamp: u32,
    pub image_size: u32,
    pub pdb_guid: [u8; 16],
    pub pdb_age: u32,
}
impl ModuleIdentity {
    pub fn validate(&self) -> Result<(), String> {
        if self.machine != 0x8664 {
            return Err("任务栏适配要求 x64 模块".into());
        }
        if self.image_size < 4096
            || self.image_size > 128 * 1024 * 1024
            || self.pdb_guid == [0; 16]
            || self.pdb_age == 0
        {
            return Err("模块身份信息无效".into());
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolRole {
    PrimarySite,
    SecondarySite,
    PrimaryHost,
    SecondaryHost,
    FrameHeight,
    ReleaseHost,
}
impl SymbolRole {
    pub const ALL: [Self; 6] = [
        Self::PrimarySite,
        Self::SecondarySite,
        Self::PrimaryHost,
        Self::SecondaryHost,
        Self::FrameHeight,
        Self::ReleaseHost,
    ];
    pub fn decorated(self) -> &'static str {
        match self {
            Self::PrimarySite => "??_7CTaskBand@@6BITaskListWndSite@@@",
            Self::SecondarySite => "??_7CSecondaryTaskBand@@6BITaskListWndSite@@@",
            Self::PrimaryHost => {
                "?GetTaskbarHost@CTaskBand@@UEBA?AV?$shared_ptr@VTaskbarHost@@@std@@XZ"
            }
            Self::SecondaryHost => {
                "?GetTaskbarHost@CSecondaryTaskBand@@UEBA?AV?$shared_ptr@VTaskbarHost@@@std@@XZ"
            }
            Self::FrameHeight => "?FrameHeight@TaskbarHost@@QEBAHXZ",
            Self::ReleaseHost => "?_Decref@_Ref_count_base@std@@QEAAXXZ",
        }
    }
    pub fn executable(self) -> bool {
        !matches!(self, Self::PrimarySite | Self::SecondarySite)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SymbolAddress {
    pub role: SymbolRole,
    pub rva: u32,
    pub section_start: u32,
    pub section_size: u32,
    pub executable: bool,
    pub prologue: Vec<u8>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostLayout {
    pub schema: u32,
    pub module: ModuleIdentity,
    pub symbols: Vec<SymbolAddress>,
    pub element_offset: u32,
}
impl HostLayout {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != VERSION {
            return Err("任务栏适配协议版本不兼容".into());
        }
        self.module.validate()?;
        if self.symbols.len() != SymbolRole::ALL.len() {
            return Err("缺少任务栏接口符号".into());
        }
        let mut seen = std::collections::BTreeSet::new();
        for symbol in &self.symbols {
            if !seen.insert(symbol.role) {
                return Err("任务栏接口符号重复".into());
            }
            let end = symbol
                .section_start
                .checked_add(symbol.section_size)
                .ok_or("模块节区溢出")?;
            let size = if symbol.role.executable() { 16 } else { 8 };
            if symbol.section_size == 0
                || end > self.module.image_size
                || symbol.rva < symbol.section_start
                || symbol.rva.checked_add(size).is_none_or(|v| v > end)
            {
                return Err("符号超出所属模块节区".into());
            }
            if symbol.executable != symbol.role.executable() {
                return Err("符号节区权限不匹配".into());
            }
            if symbol.role.executable() && symbol.prologue.len() != 16
                || !symbol.role.executable() && !symbol.prologue.is_empty()
            {
                return Err("函数入口校验数据无效".into());
            }
        }
        let offset = frame_element_offset(&self.symbol(SymbolRole::FrameHeight).prologue)?;
        if offset != self.element_offset {
            return Err("任务栏宿主字段偏移不一致".into());
        }
        Ok(())
    }
    // Call validate before accessing a role in untrusted deserialized data.
    pub fn symbol(&self, role: SymbolRole) -> &SymbolAddress {
        self.symbols
            .iter()
            .find(|s| s.role == role)
            .expect("validated required symbol")
    }
}
pub fn frame_element_offset(code: &[u8]) -> Result<u32, String> {
    if code.len() < 8 || code[..3] != [0x48, 0x83, 0xec] || code[4..7] != [0x48, 0x83, 0xc1] {
        return Err("未知的 TaskbarHost::FrameHeight 代码布局，拒绝猜测字段偏移".into());
    }
    let offset = code[7] as u32;
    if !(8..=120).contains(&offset) || !offset.is_multiple_of(8) {
        return Err("任务栏宿主字段偏移无效".into());
    }
    Ok(offset)
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InspectRequest {
    pub version: u32,
    pub nonce: u64,
    pub receiver: u64,
    pub host_pid: u32,
    pub layout: HostLayout,
}
impl InspectRequest {
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.is_empty() || bytes.len() > MAX_REQUEST {
            return Err("任务栏请求尺寸无效".into());
        }
        let request: Self = serde_json::from_slice(bytes).map_err(|_| "任务栏请求格式无效")?;
        if request.version != VERSION
            || request.nonce == 0
            || request.receiver == 0
            || request.host_pid == 0
        {
            return Err("任务栏会话身份无效".into());
        }
        request.layout.validate()?;
        Ok(request)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementInfo {
    pub name: String,
    pub class_name: String,
    pub depth: u32,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub text: Option<String>,
    #[serde(default)]
    pub visible: Option<bool>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskbarInspection {
    pub hwnd: u64,
    pub secondary: bool,
    pub elements: Vec<ElementInfo>,
    pub time_elements: Vec<String>,
    pub date_elements: Vec<String>,
    pub truncated: bool,
    pub error: Option<String>,
    #[serde(default)]
    pub topology: Option<topology::Report>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectResponse {
    pub version: u32,
    pub nonce: u64,
    pub process_id: u32,
    pub thread_id: u32,
    pub changes_applied: bool,
    pub taskbars: Vec<TaskbarInspection>,
    pub error: Option<String>,
}
impl InspectResponse {
    pub fn decode(bytes: &[u8], nonce: u64, pid: u32, tid: u32) -> Result<Self, String> {
        if bytes.is_empty() || bytes.len() > MAX_RESPONSE {
            return Err("任务栏响应尺寸无效".into());
        }
        let result: Self = serde_json::from_slice(bytes).map_err(|_| "任务栏响应格式无效")?;
        if result.version != VERSION
            || result.nonce != nonce
            || result.process_id != pid
            || result.thread_id != tid
            || result.changes_applied
        {
            return Err("任务栏响应会话或操作类型不匹配".into());
        }
        Ok(result)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    pub fn layout() -> HostLayout {
        HostLayout {
            schema: VERSION,
            module: ModuleIdentity {
                machine: 0x8664,
                timestamp: 1,
                image_size: 0x5000,
                pdb_guid: [1; 16],
                pdb_age: 1,
            },
            element_offset: 0x48,
            symbols: SymbolRole::ALL
                .into_iter()
                .enumerate()
                .map(|(i, role)| SymbolAddress {
                    role,
                    rva: 0x1000 + i as u32 * 32,
                    section_start: 0x1000,
                    section_size: 0x1000,
                    executable: role.executable(),
                    prologue: if role.executable() {
                        let mut bytes = vec![0x90; 16];
                        if role == SymbolRole::FrameHeight {
                            bytes[..8]
                                .copy_from_slice(&[0x48, 0x83, 0xec, 0x28, 0x48, 0x83, 0xc1, 0x48]);
                        }
                        bytes
                    } else {
                        vec![]
                    },
                })
                .collect(),
        }
    }
    #[test]
    fn complete_layout_is_accepted() {
        layout().validate().unwrap();
    }
    #[test]
    fn rejects_missing_duplicate_and_nonexecutable_targets() {
        let mut l = layout();
        l.symbols.pop();
        assert!(l.validate().is_err());
        let mut l = layout();
        l.symbols[0].role = SymbolRole::SecondarySite;
        assert!(l.validate().is_err());
        let mut l = layout();
        l.symbols[2].executable = false;
        assert!(l.validate().is_err());
    }
    #[test]
    fn rejects_cross_version_and_out_of_range_offsets() {
        let mut l = layout();
        l.element_offset = 0x10;
        assert!(l.validate().is_err());
        let mut l = layout();
        l.symbols[2].rva = u32::MAX;
        assert!(l.validate().is_err());
    }
    #[test]
    fn never_guesses_unknown_function_layout() {
        for code in [
            vec![],
            vec![0xe9; 16],
            vec![0x48, 0x83, 0xec, 0x28, 0x48, 0x83, 0xc1, 0xff],
            vec![0x48, 0x83, 0xec, 0x28, 0x48, 0x83, 0xc1, 7],
        ] {
            assert!(frame_element_offset(&code).is_err());
        }
    }
    #[test]
    fn validates_request_before_native_calls() {
        let r = InspectRequest {
            version: VERSION,
            nonce: 12,
            receiver: 8,
            host_pid: 4,
            layout: layout(),
        };
        let b = serde_json::to_vec(&r).unwrap();
        InspectRequest::decode(&b).unwrap();
        assert!(InspectRequest::decode(&vec![b' '; MAX_REQUEST + 1]).is_err());
        let mut v = serde_json::to_value(r).unwrap();
        v["layout"]["module"]["pdb_age"] = 0.into();
        assert!(InspectRequest::decode(&serde_json::to_vec(&v).unwrap()).is_err());
    }
    #[test]
    fn stale_and_mutating_responses_are_rejected() {
        let mut r = InspectResponse {
            version: VERSION,
            nonce: 1,
            process_id: 2,
            thread_id: 3,
            changes_applied: false,
            taskbars: vec![],
            error: None,
        };
        let b = serde_json::to_vec(&r).unwrap();
        assert!(InspectResponse::decode(&b, 2, 2, 3).is_err());
        r.changes_applied = true;
        assert!(InspectResponse::decode(&serde_json::to_vec(&r).unwrap(), 1, 2, 3).is_err());
    }
}
