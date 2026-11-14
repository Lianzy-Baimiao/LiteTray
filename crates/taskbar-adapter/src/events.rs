//! Bounded diagnostics for rare adapter events inside Explorer. Records stop
//! reasons and fatal exceptions only; never clock text, tray titles or input.
//! Windows Error Reporting may be disabled, so this is the only trace left
//! after an Explorer restart.
use std::{
    cell::UnsafeCell,
    sync::{
        OnceLock,
        atomic::{AtomicU32, AtomicUsize, Ordering},
    },
};
use windows_sys::Win32::{
    Foundation::*,
    System::{
        Diagnostics::Debug::*, LibraryLoader::*, SystemInformation::GetLocalTime, Threading::*,
    },
};

pub(crate) const FILE_NAME: &str = "adapter-events.log";
const LIMIT: u64 = 256 * 1024;
const MAX_FAULTS: u32 = 16;

/// The UTF-16 path is prepared before any fault can occur, so the exception
/// handler never allocates or reads the environment.
static PATH: OnceLock<Option<Vec<u16>>> = OnceLock::new();
static FAULTS: AtomicU32 = AtomicU32::new(0);
static OWN: OnceLock<Option<(usize, usize)>> = OnceLock::new();
static FAULT_WRITER: OnceLock<Result<Option<usize>, String>> = OnceLock::new();
static FAULT_LINES: [FaultLine; MAX_FAULTS as usize] =
    [const { FaultLine::new() }; MAX_FAULTS as usize];

/// Each exception reserves one slot for the lifetime of this pinned module.
/// Slots are never reused: publishing needs no allocation or lock, even when
/// the failing thread holds the heap lock or the event writer is stuck in I/O.
struct FaultLine {
    bytes: UnsafeCell<[u8; 1024]>,
    length: AtomicUsize,
}
// One writer owns each reserved index. The only reader acquires
// length after publication; the bytes are never modified again.
unsafe impl Sync for FaultLine {}
impl FaultLine {
    const fn new() -> Self {
        Self {
            bytes: UnsafeCell::new([0; 1024]),
            length: AtomicUsize::new(0),
        }
    }
    unsafe fn publish_once(&self, line: &[u8]) {
        let length = line.len().min(1024);
        unsafe { (&mut *self.bytes.get())[..length].copy_from_slice(&line[..length]) };
        self.length.store(length, Ordering::Release);
    }
    fn published(&self) -> Option<&[u8]> {
        let length = self.length.load(Ordering::Acquire);
        if length == 0 {
            None
        } else {
            Some(unsafe { &(&*self.bytes.get())[..length] })
        }
    }
}

fn reserve_fault(counter: &AtomicU32) -> Option<usize> {
    // The counter saturates at sixteen. Wrapping fetch_add could eventually
    // reuse a published slot, invalidating FaultLine's one-writer guarantee.
    let mut index = counter.load(Ordering::Relaxed);
    while index < MAX_FAULTS {
        match counter.compare_exchange(index, index + 1, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return Some(index as usize),
            Err(current) => index = current,
        }
    }
    None
}

fn path() -> Option<&'static [u16]> {
    PATH.get_or_init(|| {
        // Only Explorer writes the shared log. Self-tests in LiteTray.exe or
        // cargo test processes intentionally provoke failures.
        if !host_is_explorer() {
            return None;
        }
        let root = std::env::var_os("LOCALAPPDATA")?;
        let path = std::path::Path::new(&root).join("LiteTray").join(FILE_NAME);
        Some(
            path.to_string_lossy()
                .encode_utf16()
                .chain(Some(0))
                .collect(),
        )
    })
    .as_deref()
}
fn host_is_explorer() -> bool {
    let mut buffer = vec![0u16; 32768];
    let length = unsafe {
        GetModuleFileNameW(
            std::ptr::null_mut(),
            buffer.as_mut_ptr(),
            buffer.len() as u32,
        )
    } as usize;
    String::from_utf16_lossy(&buffer[..length.min(buffer.len())])
        .rsplit('\\')
        .next()
        .is_some_and(|name| name.eq_ignore_ascii_case("explorer.exe"))
}
fn local_time() -> windows_sys::Win32::Foundation::SYSTEMTIME {
    let mut time = unsafe { std::mem::zeroed() };
    unsafe { GetLocalTime(&mut time) };
    time
}
fn stamp() -> String {
    let time = local_time();
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03} pid={} tid={}",
        time.wYear,
        time.wMonth,
        time.wDay,
        time.wHour,
        time.wMinute,
        time.wSecond,
        time.wMilliseconds,
        unsafe { GetCurrentProcessId() },
        unsafe { GetCurrentThreadId() },
    )
}
/// Records an adapter event. Detail is truncated; callers pass error strings.
pub(crate) fn record(kind: &str, detail: &str) {
    #[cfg(test)]
    if !testing::enabled() {
        return;
    }
    let detail: String = detail
        .chars()
        .take(600)
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let line = format!(
        "{} v{} {kind}: {detail}\r\n",
        stamp(),
        env!("CARGO_PKG_VERSION")
    );
    #[cfg(not(test))]
    if let Some(log) = normal_log() {
        log.append(line.into_bytes());
    }
    #[cfg(test)]
    testing::append(line.into_bytes());
}

fn pin_module() -> bool {
    unsafe {
        let mut module = std::ptr::null_mut();
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_PIN,
            fault as *const u16,
            &mut module,
        ) != 0
    }
}

fn after_pin<T>(
    pin: impl FnOnce() -> bool,
    start: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    if !pin() {
        return Err("无法保持异常诊断模块生命周期".into());
    }
    start()
}

fn normal_log() -> Option<&'static litetray_event_log::Log> {
    static LOG: OnceLock<Option<litetray_event_log::Log>> = OnceLock::new();
    LOG.get_or_init(|| {
        let path = path()?;
        let path = std::path::PathBuf::from(String::from_utf16_lossy(&path[..path.len() - 1]));
        // This worker executes DLL code after a session's hook is removed.
        // Pin before publishing the worker; never rely on a caller's lease.
        after_pin(pin_module, || {
            litetray_event_log::Log::new(128, move |line| {
                let wide: Vec<u16> = format!("LiteTray adapter {}", String::from_utf8_lossy(line))
                    .encode_utf16()
                    .chain(Some(0))
                    .collect();
                unsafe { OutputDebugStringW(wide.as_ptr()) };
                let _ = litetray_event_log::write_file(&path, LIMIT, line);
            })
            .map_err(|error| error.to_string())
        })
        .ok()
    })
    .as_ref()
}

fn install_fault_writer() -> Result<(), String> {
    FAULT_WRITER
        .get_or_init(|| {
            if path().is_none() {
                return Ok(None);
            }
            let log = normal_log().ok_or("无法启动适配器诊断写入线程")?;
            let event = unsafe { CreateEventW(std::ptr::null(), 0, 0, std::ptr::null()) };
            if event.is_null() {
                return Err("无法创建异常诊断通知".into());
            }
            let signal = event as usize;
            let started = std::thread::Builder::new()
                .name("LiteTray fault observer".into())
                .spawn(move || {
                    let mut consumed = [false; MAX_FAULTS as usize];
                    while unsafe { WaitForSingleObject(signal as HANDLE, INFINITE) }
                        == WAIT_OBJECT_0
                    {
                        for (slot, consumed) in FAULT_LINES.iter().zip(&mut consumed) {
                            if !*consumed && let Some(line) = slot.published() {
                                *consumed = true;
                                // Allocations/queue delivery happen here, never
                                // in the first-chance exception callback.
                                log.append(line.to_vec());
                            }
                        }
                    }
                    unsafe { CloseHandle(signal as HANDLE) };
                });
            if let Err(error) = started {
                unsafe { CloseHandle(event) };
                return Err(format!("无法启动异常诊断通知线程：{error}"));
            }
            Ok(Some(signal))
        })
        .as_ref()
        .map(|_| ())
        .map_err(Clone::clone)
}

/// Install once per mapped module. The handler only observes: it always
/// continues the search. Its code must outlive the registration even when the
/// first taskbar preparation fails before installing any other callback.
pub(crate) fn install_fault_beacon() -> Result<(), String> {
    if !host_is_explorer() {
        return Ok(());
    }
    install_pinned_beacon()
}
fn pin_then_register(
    pin: impl FnOnce() -> bool,
    register: impl FnOnce() -> bool,
) -> Result<(), String> {
    after_pin(pin, || {
        if register() {
            Ok(())
        } else {
            Err("无法注册适配器异常诊断".into())
        }
    })
}
fn install_pinned_beacon() -> Result<(), String> {
    static INSTALLED: OnceLock<Result<(), String>> = OnceLock::new();
    INSTALLED
        .get_or_init(|| {
            let mut writer_error = None;
            let result = pin_then_register(pin_module, || {
                let _ = path();
                OWN.get_or_init(own_module);
                if let Err(error) = install_fault_writer() {
                    writer_error = Some(error);
                    return false;
                }
                unsafe { !AddVectoredExceptionHandler(0, Some(fault)).is_null() }
            });
            result.map_err(|error| writer_error.unwrap_or(error))
        })
        .clone()
}
/// The isolated loader regression shares the production registration path,
/// but does not write Explorer's log or initialize any taskbar/XAML state.
pub(crate) fn self_test_fault_beacon() -> Result<(), String> {
    if host_is_explorer() {
        return Err("异常诊断自检不能在 Explorer 中运行".into());
    }
    install_pinned_beacon()
}
const FATAL: [u32; 7] = [
    0xC000_0005, // access violation
    0xC000_001D, // illegal instruction
    0xC000_0096, // privileged instruction
    0xC000_00FD, // stack overflow
    0xC000_0374, // heap corruption
    0xC000_0602, // fail-fast exception
    0x8000_0003, // breakpoint in release code
];
unsafe extern "system" fn fault(info: *mut EXCEPTION_POINTERS) -> i32 {
    const CONTINUE_SEARCH: i32 = 0;
    unsafe {
        let Some(info) = info.as_ref() else {
            return CONTINUE_SEARCH;
        };
        let (Some(record), Some(context)) =
            (info.ExceptionRecord.as_ref(), info.ContextRecord.as_ref())
        else {
            return CONTINUE_SEARCH;
        };
        let code = record.ExceptionCode as u32;
        if !FATAL.contains(&code) {
            return CONTINUE_SEARCH;
        }
        let Some(index) = reserve_fault(&FAULTS) else {
            return CONTINUE_SEARCH;
        };
        let Some(Ok(Some(signal))) = FAULT_WRITER.get() else {
            return CONTINUE_SEARCH;
        };
        // Fixed stack buffer: the heap may be the thing that failed.
        let mut text = Line::new();
        let time = local_time();
        text.push_dec(time.wYear as u64, 4);
        text.push_str("-");
        text.push_dec(time.wMonth as u64, 2);
        text.push_str("-");
        text.push_dec(time.wDay as u64, 2);
        text.push_str(" ");
        text.push_dec(time.wHour as u64, 2);
        text.push_str(":");
        text.push_dec(time.wMinute as u64, 2);
        text.push_str(":");
        text.push_dec(time.wSecond as u64, 2);
        text.push_str(" pid=");
        text.push_dec(GetCurrentProcessId() as u64, 1);
        text.push_str(" tid=");
        text.push_dec(GetCurrentThreadId() as u64, 1);
        text.push_str(" ");
        text.push_str("first-chance exception code=");
        text.push_hex(code as u64);
        text.push_str(" at ");
        text.push_location(record.ExceptionAddress as usize);
        if code == 0xC000_0005 && record.NumberParameters >= 2 {
            text.push_str(if record.ExceptionInformation[0] == 0 {
                " read "
            } else if record.ExceptionInformation[0] == 8 {
                " execute "
            } else {
                " write "
            });
            text.push_hex(record.ExceptionInformation[1] as u64);
        }
        let own = OWN.get().copied().flatten();
        // Return addresses within this DLL near the faulting frame show
        // whether LiteTray code is involved even if the fault is in a system
        // module. Read through the kernel so a bad RSP cannot fault again.
        text.push_str(" ours=[");
        let mut found = 0;
        let mut slot = [0usize; 64];
        let mut read = 0usize;
        let rsp = context.Rsp as usize;
        let mut offset = 0usize;
        while offset < 16 * 1024 && found < 8 {
            if ReadProcessMemory(
                GetCurrentProcess(),
                (rsp + offset) as *const _,
                slot.as_mut_ptr().cast(),
                std::mem::size_of_val(&slot),
                &mut read,
            ) == 0
            {
                break;
            }
            for value in &slot[..read / 8] {
                if let Some((base, size)) = own
                    && *value >= base
                    && *value < base + size
                {
                    text.push_hex((*value - base) as u64);
                    text.push_str(" ");
                    found += 1;
                    if found >= 8 {
                        break;
                    }
                }
            }
            offset += std::mem::size_of_val(&slot);
        }
        text.push_str("]\r\n");
        FAULT_LINES[index].publish_once(text.bytes());
        SetEvent(*signal as HANDLE);
        CONTINUE_SEARCH
    }
}
fn own_module() -> Option<(usize, usize)> {
    unsafe {
        let mut module = std::ptr::null_mut();
        if GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            fault as *const u16,
            &mut module,
        ) == 0
        {
            return None;
        }
        // Our own mapped headers: e_lfanew, then OptionalHeader.SizeOfImage.
        let base = module as usize;
        let pe = *((base + 0x3c) as *const u32) as usize;
        let size = *((base + pe + 24 + 56) as *const u32) as usize;
        Some((base, size))
    }
}
/// Names the module containing an address, using only fixed buffers.
unsafe fn module_of(address: usize, name: &mut [u16; 128]) -> Option<(usize, usize)> {
    unsafe {
        let mut module = std::ptr::null_mut();
        if GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            address as *const u16,
            &mut module,
        ) == 0
        {
            return None;
        }
        let mut full = [0u16; 512];
        let length = GetModuleFileNameW(module, full.as_mut_ptr(), full.len() as u32) as usize;
        let start = full[..length]
            .iter()
            .rposition(|c| *c == b'\\' as u16)
            .map_or(0, |i| i + 1);
        let count = (length - start).min(name.len());
        name[..count].copy_from_slice(&full[start..start + count]);
        Some((module as usize, count))
    }
}
struct Line {
    buffer: [u8; 1024],
    length: usize,
}
impl Line {
    fn new() -> Self {
        Self {
            buffer: [0; 1024],
            length: 0,
        }
    }
    fn bytes(&self) -> &[u8] {
        &self.buffer[..self.length]
    }
    fn push_byte(&mut self, byte: u8) {
        if self.length < self.buffer.len() {
            self.buffer[self.length] = byte;
            self.length += 1;
        }
    }
    fn push_str(&mut self, text: &str) {
        for byte in text.bytes() {
            self.push_byte(byte);
        }
    }
    fn push_hex(&mut self, value: u64) {
        self.push_str("0x");
        let mut started = false;
        for shift in (0..16).rev() {
            let digit = ((value >> (shift * 4)) & 0xF) as u8;
            if digit != 0 || started || shift == 0 {
                started = true;
                self.push_byte(b"0123456789abcdef"[digit as usize]);
            }
        }
    }
    fn push_dec(&mut self, value: u64, width: usize) {
        let mut digits = [0u8; 20];
        let mut n = value;
        let mut count = 0;
        loop {
            digits[count] = b'0' + (n % 10) as u8;
            count += 1;
            n /= 10;
            if n == 0 {
                break;
            }
        }
        for _ in count..width {
            self.push_byte(b'0');
        }
        for i in (0..count).rev() {
            self.push_byte(digits[i]);
        }
    }
    unsafe fn push_location(&mut self, address: usize) {
        let mut name = [0u16; 128];
        match unsafe { module_of(address, &mut name) } {
            Some((base, count)) => {
                for unit in &name[..count] {
                    self.push_byte(if *unit < 0x80 { *unit as u8 } else { b'?' });
                }
                self.push_str("+");
                self.push_hex((address - base) as u64);
            }
            None => self.push_hex(address as u64),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FaultLine, Line, MAX_FAULTS, after_pin, pin_then_register, reserve_fault};
    use std::cell::Cell;

    #[test]
    fn fault_beacon_never_registers_when_module_pin_fails() {
        let registered = Cell::new(false);
        assert!(
            pin_then_register(
                || false,
                || {
                    registered.set(true);
                    true
                },
            )
            .is_err()
        );
        assert!(!registered.get());
    }

    #[test]
    fn fault_beacon_pins_before_publishing_callback_and_reports_registration_failure() {
        for accepted in [false, true] {
            let pinned = Cell::new(false);
            let result = pin_then_register(
                || {
                    pinned.set(true);
                    true
                },
                || {
                    assert!(pinned.get());
                    accepted
                },
            );
            assert_eq!(result.is_ok(), accepted);
        }
    }

    #[test]
    fn fixed_line_formats_without_allocation() {
        let mut line = Line::new();
        line.push_hex(0xC000_0005);
        line.push_str(" ");
        line.push_dec(7, 2);
        line.push_str(" ");
        line.push_hex(0);
        assert_eq!(line.bytes(), b"0xc0000005 07 0x0");
        let mut full = Line::new();
        for _ in 0..2000 {
            full.push_byte(b'x');
        }
        assert_eq!(full.bytes().len(), 1024);
    }

    #[test]
    fn event_workers_are_never_started_before_the_module_is_pinned() {
        let started = Cell::new(false);
        assert!(
            after_pin(
                || false,
                || {
                    started.set(true);
                    Ok(())
                }
            )
            .is_err()
        );
        assert!(!started.get());
        let pinned = Cell::new(false);
        after_pin(
            || {
                pinned.set(true);
                true
            },
            || {
                assert!(pinned.get());
                Ok(())
            },
        )
        .unwrap();
    }

    #[test]
    fn exception_record_is_published_without_waiting_for_the_consumer() {
        let line = FaultLine::new();
        assert!(line.published().is_none());
        unsafe { line.publish_once(b"exception while file writer is stalled") };
        assert_eq!(
            line.published().unwrap(),
            b"exception while file writer is stalled"
        );
        assert_eq!(
            line.published().unwrap(),
            b"exception while file writer is stalled"
        );
    }

    #[test]
    fn exception_slots_saturate_and_can_never_reuse_published_bytes() {
        use std::sync::atomic::{AtomicU32, Ordering};
        let counter = AtomicU32::new(0);
        for index in 0..MAX_FAULTS {
            assert_eq!(reserve_fault(&counter), Some(index as usize));
        }
        for _ in 0..100 {
            assert!(reserve_fault(&counter).is_none());
        }
        assert_eq!(counter.load(Ordering::Relaxed), MAX_FAULTS);
        counter.store(u32::MAX, Ordering::Relaxed);
        assert!(reserve_fault(&counter).is_none());
        assert_eq!(counter.load(Ordering::Relaxed), u32::MAX);
    }

    #[test]
    fn concurrent_faulting_threads_reserve_disjoint_slots() {
        use std::sync::{Arc, Barrier, atomic::AtomicU32};
        let counter = Arc::new(AtomicU32::new(0));
        let start = Arc::new(Barrier::new(9));
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let counter = counter.clone();
                let start = start.clone();
                std::thread::spawn(move || {
                    start.wait();
                    [
                        reserve_fault(&counter).unwrap(),
                        reserve_fault(&counter).unwrap(),
                    ]
                })
            })
            .collect();
        start.wait();
        let mut slots: Vec<_> = threads
            .into_iter()
            .flat_map(|thread| thread.join().unwrap())
            .collect();
        slots.sort_unstable();
        assert_eq!(slots, (0..MAX_FAULTS as usize).collect::<Vec<_>>());
    }

    #[test]
    fn taskbar_event_returns_while_its_real_log_dispatcher_waits_in_the_sink() {
        use std::{
            sync::{Arc, mpsc},
            time::Duration,
        };
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (returned_tx, returned_rx) = mpsc::channel();
        let (written_tx, written_rx) = mpsc::channel();
        let taskbar = std::thread::spawn(move || {
            let mut gate = Some(release_rx);
            let log = Arc::new(
                litetray_event_log::Log::new(4, move |line| {
                    if let Some(gate) = gate.take() {
                        let _ = entered_tx.send(());
                        let _ = gate.recv_timeout(Duration::from_secs(3));
                    }
                    let _ = written_tx.send(line.to_vec());
                })
                .unwrap(),
            );
            super::testing::with_log(log, || {
                super::record("session poll stopped", "native clock lost")
            });
            let _ = returned_tx.send(());
        });
        let entered = entered_rx.recv_timeout(Duration::from_secs(3)).is_ok();
        let returned = returned_rx.recv_timeout(Duration::from_millis(250)).is_ok();
        let _ = release_tx.send(());
        taskbar.join().unwrap();
        assert!(
            entered,
            "the production adapter event formatter must reach the sink"
        );
        assert!(
            returned,
            "Explorer's event caller must not wait for log I/O"
        );
        let line = written_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        assert!(
            String::from_utf8(line)
                .unwrap()
                .contains("session poll stopped: native clock lost")
        );
    }
}

#[cfg(test)]
mod testing {
    use std::{cell::RefCell, sync::Arc};
    thread_local! {
        static LOG: RefCell<Option<Arc<litetray_event_log::Log>>> = const { RefCell::new(None) };
    }
    pub(super) fn enabled() -> bool {
        LOG.with(|log| log.borrow().is_some())
    }
    pub(super) fn append(line: Vec<u8>) {
        LOG.with(|log| {
            if let Some(log) = log.borrow().as_ref() {
                log.append(line);
            }
        });
    }
    pub(super) fn with_log<T>(log: Arc<litetray_event_log::Log>, action: impl FnOnce() -> T) -> T {
        struct Restore(Option<Arc<litetray_event_log::Log>>);
        impl Drop for Restore {
            fn drop(&mut self) {
                LOG.with(|log| *log.borrow_mut() = self.0.take());
            }
        }
        let _restore = Restore(LOG.with(|slot| slot.replace(Some(log))));
        action()
    }
}
