//! Narrow QQ hover-card routing lease. Empty targets revoke the lease.
use serde::{Deserialize, Serialize};
pub const TAG: usize = 0x4c545051;
pub const MAX_BYTES: usize = 2048;
pub const HOST_PROPERTY: &str = "LiteTray.PopupHook.Host.v1";
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub version: u32,
    pub host: u64,
    pub host_pid: u32,
    pub taskbars: Vec<u64>,
}
impl Request {
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.is_empty() || bytes.len() > MAX_BYTES {
            return None;
        }
        let request: Self = serde_json::from_slice(bytes).ok()?;
        (request.version == 1
            && request.host != 0
            && request.host_pid != 0
            && request.taskbars.len() <= 16
            && request.taskbars.iter().all(|&h| h != 0))
        .then_some(request)
    }
}
