//! UI-thread lifetime for a set of native clocks. The host cannot keep an old
//! session alive with replayed messages or mutate a partly restored session.
use crate::clock::ClockLease;
use crate::{
    input::{Probe, Sequence, Subscription},
    tray::{ModelLease, Pair},
    xaml::Object,
};
use litetray_adapter_protocol::activation::Activation;
use litetray_adapter_protocol::clock::{Change, Command, Frame, Lease, MAX_CLOCKS, Owner};
use std::rc::Rc;
use windows_sys::Win32::Foundation::HWND;

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
/// Caller has verified that this island HWND and element share one XAML root.
pub struct InputTarget {
    pub id: u64,
    pub element: Object,
    pub island: HWND,
}
pub struct Session {
    lease: Lease,
    targets: Vec<Target>,
    state: State,
    frame: Option<Frame>,
    inputs: Vec<(u64, Subscription)>,
    inputs_prepared: bool,
    tray: Option<ModelLease>,
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
            inputs: vec![],
            inputs_prepared: false,
            tray: None,
        })
    }
    pub fn state(&self) -> State {
        self.state
    }
    pub fn owner(&self) -> &Owner {
        self.lease.owner()
    }
    pub fn accepts_input(&self, now: u64) -> bool {
        self.state == State::Active && !self.lease.expired(now)
    }
    /// Build every subscription while inactive. Partial preparation is rolled
    /// back before any text/style is changed, retaining failed detaches.
    pub fn prepare_inputs(
        &mut self,
        targets: Vec<InputTarget>,
        publish: impl Fn(Activation) -> bool + 'static,
    ) -> Result<(), String> {
        if self.state != State::Prepared || self.inputs_prepared {
            return Err("点击订阅只能在会话准备阶段配置一次".into());
        }
        if targets.len() > MAX_CLOCKS
            || targets.iter().enumerate().any(|(i, t)| {
                !self.targets.iter().any(|clock| clock.id == t.id)
                    || targets[..i].iter().any(|old| old.id == t.id)
            })
        {
            return Err("点击目标不属于当前时钟会话或存在重复".into());
        }
        // Two target IDs must not subscribe to the same native input element.
        let mut identities = Vec::new();
        for target in &targets {
            let id = target.element.query(&windows_sys::core::IID_IUnknown)?;
            if identities
                .iter()
                .any(|other: &Object| other.raw() == id.raw())
            {
                return Err("点击控件被多个时钟重复引用".into());
            }
            identities.push(id);
        }
        self.inputs_prepared = true;
        let sequence = Sequence::new();
        let publish = Rc::new(publish);
        for target in targets {
            let publisher = publish.clone();
            match Subscription::prepare(
                &target.element,
                target.island,
                self.owner().clone(),
                target.id,
                sequence.clone(),
                move |event| publisher(event),
            ) {
                Ok(input) => self.inputs.push((target.id, input)),
                Err(error) => {
                    return match self.stop() {
                        Ok(()) => Err(error),
                        Err(restore) => Err(format!("{error}；{restore}")),
                    };
                }
            }
        }
        Ok(())
    }
    pub(crate) fn input_probe(&self, id: u64) -> Result<Probe, String> {
        self.inputs
            .iter()
            .find(|(target, _)| *target == id)
            .ok_or("点击测试目标不存在")?
            .1
            .probe()
    }
    /// Tray properties participate in the same start/stop lifetime as clocks.
    pub(crate) fn prepare_tray(&mut self, pairs: Vec<Pair>) -> Result<(), String> {
        if self.state != State::Prepared || self.tray.is_some() {
            return Err("托盘模型只能在准备阶段配置一次".into());
        }
        for pair in &pairs {
            for target in &self.targets {
                if target.clock.references(&pair.source)?
                    || target.clock.references(&pair.target)?
                {
                    return Err("托盘模型控件与时钟目标重叠".into());
                }
            }
        }
        self.tray = Some(ModelLease::capture(pairs)?);
        Ok(())
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
        if let Some(tray) = &mut self.tray
            && let Err(error) = tray.start()
        {
            return match self.stop() {
                Ok(()) => Err(error),
                Err(restore) => Err(format!("{error}；{restore}")),
            };
        }
        for (_, input) in &self.inputs {
            if let Err(error) = input.activate() {
                return match self.stop() {
                    Ok(()) => Err(error),
                    Err(restore) => Err(format!("{error}；{restore}")),
                };
            }
        }
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
        if self.state == State::Active
            && let Some(tray) = &mut self.tray
        {
            // A failed model refresh rolls back its group synchronously.
            // Suspend input before any of those COM calls can reenter it.
            for (_, input) in &self.inputs {
                input.suspend();
            }
            if let Err(error) = tray.refresh() {
                return match self.stop() {
                    Ok(()) => Err(error),
                    Err(restore) => Err(format!("{error}；{restore}")),
                };
            }
            for (_, input) in &self.inputs {
                if let Err(error) = input.activate() {
                    return match self.stop() {
                        Ok(()) => Err(error),
                        Err(restore) => Err(format!("{error}；{restore}")),
                    };
                }
            }
        }
        Ok(())
    }
    pub fn stop(&mut self) -> Result<(), String> {
        if self.state == State::Closed {
            return Ok(());
        }
        self.state = State::Restoring;
        // Stop every delegate before the first COM detach/restore may pump
        // messages and reenter another clock's callback.
        for (_, input) in &self.inputs {
            input.suspend();
        }
        let mut errors = Vec::new();
        for (id, input) in self.inputs.iter_mut().rev() {
            if let Err(error) = input.detach() {
                errors.push(format!("时钟 {id} 点击恢复：{error}"));
            }
        }
        if let Some(tray) = &mut self.tray
            && let Err(error) = tray.stop()
        {
            errors.push(format!("托盘模型恢复：{error}"));
        }
        for target in self.targets.iter_mut().rev() {
            if let Err(error) = target.clock.restore() {
                errors.push(format!("时钟 {}：{error}", target.id));
            }
        }
        self.frame = None;
        if errors.is_empty() {
            self.inputs.clear();
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
