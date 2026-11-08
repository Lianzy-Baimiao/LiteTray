//! Bounded diagnostics for rare adapter events inside Explorer. Records stop
//! reasons and fatal exceptions only; never clock text, tray titles or input.
//! Windows Error Reporting may be disabled, so this is the only trace left
//! after an Explorer restart.
use std::{
    io::Write,
    sync::{
        OnceLock,
        atomic::{AtomicU32, Ordering},
    },
};
use windows_sys::Win32::{
    Foundation::*,
    Storage::FileSystem::*,
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
    let wide: Vec<u16> = format!("LiteTray adapter {line}")
        .encode_utf16()
        .chain(Some(0))
        .collect();
    unsafe { OutputDebugStringW(wide.as_ptr()) };
    let Some(path) = path() else {
        return;
    };
    let path = String::from_utf16_lossy(&path[..path.len() - 1]);
    let path = std::path::Path::new(&path);
    if std::fs::metadata(path).is_ok_and(|m| m.len() > LIMIT) {
        let _ = std::fs::rename(path, path.with_extension("old.log"));
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = file.write_all(line.as_bytes());
    }
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
    if !pin() {
        return Err("无法保持异常诊断模块生命周期".into());
    }
    if !register() {
        return Err("无法注册适配器异常诊断".into());
    }
    Ok(())
}
fn install_pinned_beacon() -> Result<(), String> {
    static INSTALLED: OnceLock<Result<(), String>> = OnceLock::new();
    INSTALLED
        .get_or_init(|| {
            pin_then_register(
                || unsafe {
                    let mut module = std::ptr::null_mut();
                    GetModuleHandleExW(
                        GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_PIN,
                        fault as *const u16,
                        &mut module,
                    ) != 0
                },
                || {
                    let _ = path();
                    OWN.get_or_init(own_module);
                    unsafe { !AddVectoredExceptionHandler(0, Some(fault)).is_null() }
                },
            )
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
        if !FATAL.contains(&code) || FAULTS.fetch_add(1, Ordering::Relaxed) >= MAX_FAULTS {
            return CONTINUE_SEARCH;
        }
        let Some(Some(path)) = PATH.get() else {
            return CONTINUE_SEARCH;
        };
        // Fixed stack buffer: the heap may be the thing that failed.
        let mut text = Line::new();
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
        let file = CreateFileW(
            path.as_ptr(),
            FILE_APPEND_DATA,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_ALWAYS,
            FILE_ATTRIBUTE_NORMAL,
            std::ptr::null_mut(),
        );
        if file != INVALID_HANDLE_VALUE {
            let mut written = 0;
            let time = local_time();
            let mut prefix = Line::new();
            prefix.push_dec(time.wYear as u64, 4);
            prefix.push_str("-");
            prefix.push_dec(time.wMonth as u64, 2);
            prefix.push_str("-");
            prefix.push_dec(time.wDay as u64, 2);
            prefix.push_str(" ");
            prefix.push_dec(time.wHour as u64, 2);
            prefix.push_str(":");
            prefix.push_dec(time.wMinute as u64, 2);
            prefix.push_str(":");
            prefix.push_dec(time.wSecond as u64, 2);
            prefix.push_str(" pid=");
            prefix.push_dec(GetCurrentProcessId() as u64, 1);
            prefix.push_str(" tid=");
            prefix.push_dec(GetCurrentThreadId() as u64, 1);
            prefix.push_str(" ");
            WriteFile(
                file,
                prefix.bytes().as_ptr(),
                prefix.bytes().len() as u32,
                &mut written,
                std::ptr::null_mut(),
            );
            WriteFile(
                file,
                text.bytes().as_ptr(),
                text.bytes().len() as u32,
                &mut written,
                std::ptr::null_mut(),
            );
            CloseHandle(file);
        }
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
    use super::{Line, pin_then_register};
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
}
