//! Clock sessions exchange formatted text only. Formatting, weather and disk
//! access remain in the host. Session ownership is verified by the transport.
use crate::{MAX_REQUEST, VERSION};
use serde::{Deserialize, Serialize};

pub const LEASE_MILLIS: u64 = 15_000;
pub const HEARTBEAT_MILLIS: u64 = 5_000;
pub const MAX_CLOCKS: usize = 16;
/// WM_COPYDATA tag for commands to an already established clock session.
/// A nonzero native return value acknowledges application; zero rejects it.
pub const COMMAND_TAG: usize = 0x4c544103;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Owner {
    pub nonce: u64,
    pub process_id: u32,
    pub receiver: u64,
}
impl Owner {
    pub fn validate(&self) -> Result<(), String> {
        if self.nonce == 0 || self.process_id == 0 || self.receiver == 0 {
            Err("时钟会话身份不完整".into())
        } else {
            Ok(())
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Text {
    pub top: String,
    pub bottom: String,
}
impl Text {
    pub fn validate(&self) -> Result<(), String> {
        if self.top.len() > 4096
            || self.bottom.len() > 4096
            || self.top.contains('\0')
            || self.bottom.contains('\0')
        {
            Err("时钟文本过长或包含无效字符".into())
        } else {
            Ok(())
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Style {
    pub font_family: String,
    pub font_size: u32,
    pub width: u32,
}
impl Style {
    pub fn validate(&self) -> Result<(), String> {
        if !(8..=32).contains(&self.font_size)
            || !(80..=600).contains(&self.width)
            || self.font_family.trim().is_empty()
            || self.font_family.len() > 128
            || self.font_family.contains('\0')
        {
            Err("时钟样式参数无效".into())
        } else {
            Ok(())
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Frame {
    pub text: Text,
    pub style: Style,
}
impl Frame {
    pub fn validate(&self) -> Result<(), String> {
        self.text.validate()?;
        self.style.validate()
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Change {
    Update { frame: Frame },
    Tick { text: Text },
    Heartbeat,
    Stop,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub version: u32,
    pub owner: Owner,
    pub sequence: u64,
    pub change: Change,
}
impl Command {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != VERSION || self.sequence == 0 {
            return Err("时钟命令版本或序号无效".into());
        }
        self.owner.validate()?;
        match &self.change {
            Change::Update { frame } => frame.validate(),
            Change::Tick { text } => text.validate(),
            _ => Ok(()),
        }
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.is_empty() || bytes.len() > MAX_REQUEST {
            return Err("时钟命令尺寸无效".into());
        }
        let command: Self = serde_json::from_slice(bytes).map_err(|_| "时钟命令格式无效")?;
        command.validate()?;
        Ok(command)
    }
}
/// Monotonic sequencing and lease expiry, independent of the Windows clock.
/// A command is committed only after every target accepted the update.
pub struct Lease {
    owner: Owner,
    sequence: u64,
    deadline: u64,
    last_now: u64,
}
impl Lease {
    pub fn owner(&self) -> &Owner {
        &self.owner
    }
    pub fn new(owner: Owner, now: u64) -> Result<Self, String> {
        owner.validate()?;
        let deadline = now.checked_add(LEASE_MILLIS).ok_or("时钟会话时间溢出")?;
        Ok(Self {
            owner,
            sequence: 0,
            deadline,
            last_now: now,
        })
    }
    pub fn expired(&self, now: u64) -> bool {
        now < self.last_now || now >= self.deadline
    }
    pub fn check(&self, command: &Command, now: u64) -> Result<(), String> {
        command.validate()?;
        if command.owner != self.owner {
            return Err("时钟会话不匹配".into());
        }
        if command.sequence <= self.sequence {
            return Err("忽略重复或乱序时钟命令".into());
        }
        if self.expired(now) {
            return Err("时钟会话已过期".into());
        }
        now.checked_add(LEASE_MILLIS).ok_or("时钟会话时间溢出")?;
        Ok(())
    }
    pub fn commit(&mut self, command: &Command, now: u64) -> Result<(), String> {
        self.check(command, now)?;
        self.sequence = command.sequence;
        self.deadline = now + LEASE_MILLIS;
        self.last_now = now;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn owner() -> Owner {
        Owner {
            nonce: 1,
            process_id: 2,
            receiver: 3,
        }
    }
    fn command(sequence: u64) -> Command {
        Command {
            version: VERSION,
            owner: owner(),
            sequence,
            change: Change::Heartbeat,
        }
    }
    #[test]
    fn replay_and_other_owners_do_not_extend_a_lease() {
        let mut lease = Lease::new(owner(), 100).unwrap();
        lease.commit(&command(3), 200).unwrap();
        assert!(lease.commit(&command(2), 300).is_err());
        assert!(lease.commit(&command(3), 400).is_err());
        let mut other = command(4);
        other.owner.nonce += 1;
        assert!(lease.commit(&other, 500).is_err());
        assert!(!lease.expired(15199));
        assert!(lease.expired(15200));
    }
    #[test]
    fn expiry_and_monotonic_time_fail_closed() {
        let mut lease = Lease::new(owner(), 1000).unwrap();
        assert!(lease.commit(&command(1), 999).is_err());
        assert!(lease.commit(&command(1), 16000).is_err());
        assert!(Lease::new(owner(), u64::MAX).is_err());
        assert!(lease.commit(&command(1), 15999).is_ok());
    }
    #[test]
    fn decoding_limits_size_and_refuses_unknown_operations_or_fields() {
        assert!(Command::decode(&serde_json::to_vec(&command(1)).unwrap()).is_ok());
        assert!(Command::decode(&vec![b' '; MAX_REQUEST + 1]).is_err());
        let mut v = serde_json::to_value(command(1)).unwrap();
        v["change"]["operation"] = "start_shell".into();
        assert!(Command::decode(&serde_json::to_vec(&v).unwrap()).is_err());
        v["change"]["operation"] = "heartbeat".into();
        v["execute"] = "arbitrary".into();
        assert!(Command::decode(&serde_json::to_vec(&v).unwrap()).is_err());
    }
    #[test]
    fn valid_text_cannot_carry_unbounded_or_nul_content() {
        assert!(
            Text {
                top: "当前天气 24°C".into(),
                bottom: "".into()
            }
            .validate()
            .is_ok()
        );
        assert!(
            Text {
                top: "a\0b".into(),
                bottom: "".into()
            }
            .validate()
            .is_err()
        );
        assert!(
            Style {
                font_family: "".into(),
                font_size: 12,
                width: 200
            }
            .validate()
            .is_err()
        );
    }
}
