//! Cold-navigation diagnostics, compiled only for tests and enabled explicitly
//! by the latency check. Timings are inclusive; nested phases must not be added.
use std::{cell::RefCell, time::Instant};
use windows_sys::Win32::{
    Foundation::FILETIME,
    System::Threading::{GetCurrentThread, GetThreadTimes},
};

#[derive(Clone, Copy)]
pub(super) enum Phase {
    Command,
    CollectPage,
    Render,
    StyleControls,
    FontEnumeration,
    ControlCreateAndStyle,
    ComboPopulation,
    PageFinish,
    UpdateViews,
    ShowPage,
    SynchronousPaint,
}

const PHASES: [(Phase, &str); 11] = [
    (Phase::Command, "command"),
    (Phase::CollectPage, "collect_page"),
    (Phase::Render, "render"),
    (Phase::StyleControls, "style_controls"),
    (Phase::FontEnumeration, "font_enumeration"),
    (Phase::ControlCreateAndStyle, "control_create_and_style"),
    (Phase::ComboPopulation, "combo_population"),
    (Phase::PageFinish, "page_finish"),
    (Phase::UpdateViews, "update_views"),
    (Phase::ShowPage, "show_page"),
    (Phase::SynchronousPaint, "synchronous_paint"),
];

#[derive(Clone, Copy)]
struct Measurement {
    calls: usize,
    wall_us: u128,
    cpu_100ns: Option<u64>,
}

const EMPTY: Measurement = Measurement {
    calls: 0,
    wall_us: 0,
    cpu_100ns: Some(0),
};

thread_local! {
    static RECORDING: RefCell<Option<[Measurement; PHASES.len()]>> = const { RefCell::new(None) };
}

struct Start {
    wall: Instant,
    cpu_100ns: Option<u64>,
}

impl Start {
    fn now() -> Self {
        Self {
            wall: Instant::now(),
            cpu_100ns: thread_cpu_100ns(),
        }
    }

    fn elapsed(&self) -> Measurement {
        let wall_us = self.wall.elapsed().as_micros();
        let cpu_100ns = self
            .cpu_100ns
            .zip(thread_cpu_100ns())
            .and_then(|(start, end)| end.checked_sub(start));
        Measurement {
            calls: 1,
            wall_us,
            cpu_100ns,
        }
    }
}

fn thread_cpu_100ns() -> Option<u64> {
    unsafe {
        let mut created: FILETIME = std::mem::zeroed();
        let mut exited: FILETIME = std::mem::zeroed();
        let mut kernel: FILETIME = std::mem::zeroed();
        let mut user: FILETIME = std::mem::zeroed();
        if GetThreadTimes(
            GetCurrentThread(),
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        ) == 0
        {
            return None;
        }
        let ticks =
            |time: FILETIME| (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime);
        Some(ticks(kernel) + ticks(user))
    }
}

pub(super) struct Recording(Start);

pub(super) fn begin() -> Recording {
    RECORDING.with(|recording| {
        assert!(recording.replace(Some([EMPTY; PHASES.len()])).is_none());
    });
    Recording(Start::now())
}

impl Recording {
    pub(super) fn finish(self) -> Report {
        let total = self.0.elapsed();
        let phases = RECORDING.with(|recording| recording.take().unwrap());
        Report { total, phases }
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        RECORDING.with(|recording| recording.take());
    }
}

pub(super) struct Scope {
    phase: Phase,
    start: Start,
}

pub(super) fn scope(phase: Phase) -> Option<Scope> {
    RECORDING
        .with(|recording| recording.borrow().is_some())
        .then(|| Scope {
            phase,
            start: Start::now(),
        })
}

impl Drop for Scope {
    fn drop(&mut self) {
        let elapsed = self.start.elapsed();
        RECORDING.with(|recording| {
            if let Some(phases) = recording.borrow_mut().as_mut() {
                let phase = &mut phases[self.phase as usize];
                phase.calls += 1;
                phase.wall_us += elapsed.wall_us;
                phase.cpu_100ns = phase
                    .cpu_100ns
                    .zip(elapsed.cpu_100ns)
                    .map(|(total, elapsed)| total + elapsed);
            }
        });
    }
}

pub(super) struct Report {
    total: Measurement,
    phases: [Measurement; PHASES.len()],
}

impl Report {
    pub(super) fn print(&self) {
        eprintln!(
            "clock/settings cold phases: inclusive timings; GetThreadTimes CPU accounting can be coarse; instrumentation overhead is included"
        );
        let print = |name: &str, measurement: Measurement| {
            let cpu = measurement
                .cpu_100ns
                .map(|ticks| format!("{}us", ticks / 10))
                .unwrap_or_else(|| "unavailable".into());
            eprintln!(
                "  {name}: calls={}, wall={}us, thread_cpu={cpu}",
                measurement.calls, measurement.wall_us
            );
        };
        print("total", self.total);
        for (phase, name) in PHASES {
            let measurement = self.phases[phase as usize];
            if measurement.calls != 0 {
                print(name, measurement);
            }
        }
    }
}
