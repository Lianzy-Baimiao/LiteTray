//! Clock activation carries physical screen coordinates captured at the input
//! event. The host must not substitute its current mouse position.
use crate::{VERSION, clock::Owner};
use serde::{Deserialize, Serialize};
pub const EVENT_TAG: usize = 0x4c544104;
pub const MAX_EVENT: usize = 2048;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}
impl Rect {
    pub fn validate(&self) -> Result<(), String> {
        let width = i64::from(self.right) - i64::from(self.left);
        let height = i64::from(self.bottom) - i64::from(self.top);
        if !(1..=32768).contains(&width) || !(1..=32768).contains(&height) {
            return Err("时钟点击矩形无效".into());
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Input {
    Pointer,
    Keyboard,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Activation {
    pub version: u32,
    pub owner: Owner,
    pub sequence: u64,
    pub clock_id: u64,
    pub source_window: u64,
    pub anchor: Rect,
    pub input: Input,
}
impl Activation {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != VERSION
            || self.sequence == 0
            || self.clock_id == 0
            || self.source_window == 0
        {
            return Err("时钟点击身份无效".into());
        }
        self.owner.validate()?;
        self.anchor.validate()
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.is_empty() || bytes.len() > MAX_EVENT {
            return Err("时钟点击数据尺寸无效".into());
        }
        let event: Self = serde_json::from_slice(bytes).map_err(|_| "时钟点击数据格式无效")?;
        event.validate()?;
        Ok(event)
    }
}

/// Host-side gate. The transport first verifies the sender HWND/PID against
/// the established session, then commits only events for its known clocks.
pub struct Gate {
    owner: Owner,
    targets: Vec<(u64, u64)>,
    sequence: u64,
}
impl Gate {
    pub fn new(owner: Owner, targets: Vec<(u64, u64)>) -> Result<Self, String> {
        owner.validate()?;
        if targets.is_empty()
            || targets.len() > crate::clock::MAX_CLOCKS
            || targets
                .iter()
                .enumerate()
                .any(|(i, t)| t.0 == 0 || t.1 == 0 || targets[..i].iter().any(|old| old.0 == t.0))
        {
            return Err("时钟点击目标无效".into());
        }
        Ok(Self {
            owner,
            targets,
            sequence: 0,
        })
    }
    pub fn accept(&mut self, bytes: &[u8]) -> Result<Activation, String> {
        let event = Activation::decode(bytes)?;
        if event.owner != self.owner
            || event.sequence <= self.sequence
            || !self
                .targets
                .contains(&(event.clock_id, event.source_window))
        {
            return Err("时钟点击会话、目标或序号不匹配".into());
        }
        self.sequence = event.sequence;
        Ok(event)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn event() -> Activation {
        Activation {
            version: VERSION,
            owner: Owner {
                nonce: 1,
                process_id: 2,
                receiver: 3,
            },
            sequence: 1,
            clock_id: 4,
            source_window: 5,
            anchor: Rect {
                left: -1800,
                top: -60,
                right: -1600,
                bottom: -10,
            },
            input: Input::Keyboard,
        }
    }
    #[test]
    fn preserves_signed_coordinates_and_rejects_replay_and_wrong_targets() {
        let mut e = event();
        let mut gate = Gate::new(e.owner.clone(), vec![(4, 5)]).unwrap();
        let bytes = serde_json::to_vec(&e).unwrap();
        assert_eq!(gate.accept(&bytes).unwrap().anchor, e.anchor);
        assert!(gate.accept(&bytes).is_err());
        e.sequence = 2;
        e.source_window = 6;
        assert!(gate.accept(&serde_json::to_vec(&e).unwrap()).is_err());
        e.source_window = 5;
        e.owner.nonce += 1;
        assert!(gate.accept(&serde_json::to_vec(&e).unwrap()).is_err());
    }
    #[test]
    fn rejects_invalid_and_overflowing_bounds_without_panics() {
        let mut e = event();
        e.anchor.right = e.anchor.left;
        assert!(e.validate().is_err());
        e.anchor.left = i32::MIN;
        e.anchor.right = i32::MAX;
        assert!(e.validate().is_err());
        assert!(Activation::decode(&vec![b' '; MAX_EVENT + 1]).is_err());
        let mut json = serde_json::to_value(event()).unwrap();
        json["execute"] = "anything".into();
        assert!(Activation::decode(&serde_json::to_vec(&json).unwrap()).is_err());
    }
}
