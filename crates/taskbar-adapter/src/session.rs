//! UI-thread lifetime for a set of native clocks. The host cannot keep an old
//! session alive with replayed messages or mutate a partly restored session.
use crate::clock::ClockLease;
use litetray_adapter_protocol::clock::{Change, Command, Frame, Lease, MAX_CLOCKS, Owner};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Prepared,
    Active,
    Restoring,
    Closed,
}
pub struct Target {
    pub id: u64,
    pub clock: ClockLease,
}
pub struct Session {
    lease: Lease,
    targets: Vec<Target>,
    state: State,
    frame: Option<Frame>,
}
impl Session {
    /// Captured targets must be disjoint; otherwise rollback order would
    /// overwrite another clock's original properties.
    pub fn new(owner: Owner, targets: Vec<Target>, now: u64) -> Result<Self, String> {
        if targets.is_empty() || targets.len() > MAX_CLOCKS {
            return Err("时钟目标数量无效".into());
        }
        for (i, target) in targets.iter().enumerate() {
            if target.id == 0
                || !target.clock.clean()
                || targets[..i]
                    .iter()
                    .any(|t| t.id == target.id || t.clock.overlaps(&target.clock))
            {
                return Err("时钟目标重复、重叠或尚未恢复".into());
            }
        }
        Ok(Self {
            lease: Lease::new(owner, now)?,
            targets,
            state: State::Prepared,
            frame: None,
        })
    }
    pub fn state(&self) -> State {
        self.state
    }
    pub fn owner(&self) -> &Owner {
        self.lease.owner()
    }
    /// On failure the caller retains this session to retry unfinished restores.
    pub fn start(&mut self, frame: Frame, now: u64) -> Result<(), String> {
        if self.state != State::Prepared {
            return Err("时钟会话已启动或已关闭".into());
        }
        frame.validate()?;
        if self.lease.expired(now) {
            self.stop()?;
            return Err("时钟初始化已超时".into());
        }
        self.apply(frame)?;
        self.state = State::Active;
        Ok(())
    }
    pub fn receive(&mut self, bytes: &[u8], now: u64) -> Result<(), String> {
        let command = Command::decode(bytes)?;
        if self.state != State::Active {
            return Err("时钟会话不再接受更新".into());
        }
        if self.lease.expired(now) {
            self.stop()?;
            return Err("时钟会话已过期并恢复".into());
        }
        self.lease.check(&command, now)?;
        match &command.change {
            Change::Update { frame } => self.apply(frame.clone())?,
            Change::Tick { text } => {
                let mut frame = self.frame.clone().ok_or("时钟快照缺失")?;
                frame.text = text.clone();
                self.apply(frame)?;
            }
            Change::Heartbeat => {}
            Change::Stop => return self.stop(),
        }
        self.lease.commit(&command, now)
    }
    fn apply(&mut self, frame: Frame) -> Result<(), String> {
        let style_changed = self
            .frame
            .as_ref()
            .is_none_or(|old| old.style != frame.style);
        for target in &mut self.targets {
            let result = if style_changed {
                target.clock.apply(
                    &frame.text.top,
                    &frame.text.bottom,
                    frame.style.font_size as f64,
                    &frame.style.font_family,
                    frame.style.width as f64,
                )
            } else {
                target.clock.tick(&frame.text.top, &frame.text.bottom)
            };
            if let Err(error) = result {
                let error = format!("时钟 {} 更新失败：{error}", target.id);
                return match self.stop() {
                    Ok(()) => Err(error),
                    Err(restore) => Err(format!("{error}；{restore}")),
                };
            }
        }
        self.frame = Some(frame);
        Ok(())
    }
    /// Called from the owning UI thread by its native timer. `host_alive`
    /// comes from a retained process handle, not a potentially reused PID.
    pub fn poll(&mut self, now: u64, host_alive: bool) -> Result<(), String> {
        if self.state == State::Restoring
            || self.state != State::Closed && (!host_alive || self.lease.expired(now))
        {
            self.stop()?;
        }
        Ok(())
    }
    pub fn stop(&mut self) -> Result<(), String> {
        if self.state == State::Closed {
            return Ok(());
        }
        self.state = State::Restoring;
        let mut errors = Vec::new();
        for target in self.targets.iter_mut().rev() {
            if let Err(error) = target.clock.restore() {
                errors.push(format!("时钟 {}：{error}", target.id));
            }
        }
        self.frame = None;
        if errors.is_empty() {
            self.targets.clear();
            self.state = State::Closed;
            Ok(())
        } else {
            Err(format!("恢复未完成：{}", errors.join("；")))
        }
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
