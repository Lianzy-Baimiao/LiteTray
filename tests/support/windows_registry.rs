use std::{ptr, time::Duration};
use windows_sys::Win32::{Foundation::*, System::Registry::*};

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

/// Run process-wide registry/environment changes in a dedicated test child.
#[allow(dead_code)] // The native UI target provides its own isolated child.
pub fn in_child(check: impl FnOnce()) {
    const CHILD: &str = "LITETRAY_ISOLATED_REGISTRY_TEST";
    if std::env::var_os(CHILD).is_some() {
        check();
        return;
    }
    use std::{os::windows::process::CommandExt, process::Command};
    let name = std::thread::current().name().unwrap().to_owned();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &name, "--test-threads=1", "--nocapture"])
        .env(CHILD, "1")
        .creation_flags(0x08000000)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while child.try_wait().unwrap().is_none() {
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("isolated registry test timed out: {name}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "isolated registry test failed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
}

/// Redirect HKCU only in this child process; the user's real settings stay intact.
pub struct CurrentUser {
    original: HKEY,
    sandbox: HKEY,
    path: Vec<u16>,
}

impl CurrentUser {
    pub fn new() -> Self {
        assert!(
            std::env::var_os("LITETRAY_ISOLATED_REGISTRY_TEST").is_some()
                || std::env::var_os("LITETRAY_ISOLATED_SETTINGS_TEST").is_some()
        );
        unsafe {
            let mut original = ptr::null_mut();
            assert_eq!(
                RegOpenCurrentUser(KEY_ALL_ACCESS, &mut original),
                ERROR_SUCCESS
            );
            let path = wide(&format!(
                "Software\\LiteTray.Tests\\registry-{}-{}",
                std::process::id(),
                chrono::Utc::now().timestamp_nanos_opt().unwrap()
            ));
            let mut sandbox = ptr::null_mut();
            assert_eq!(
                RegCreateKeyExW(
                    original,
                    path.as_ptr(),
                    0,
                    ptr::null(),
                    REG_OPTION_VOLATILE,
                    KEY_ALL_ACCESS,
                    ptr::null(),
                    &mut sandbox,
                    ptr::null_mut()
                ),
                ERROR_SUCCESS
            );
            assert_eq!(
                RegOverridePredefKey(HKEY_CURRENT_USER, sandbox),
                ERROR_SUCCESS
            );
            Self {
                original,
                sandbox,
                path,
            }
        }
    }

    pub fn write(&self, path: &str, name: &str, kind: REG_VALUE_TYPE, bytes: &[u8]) {
        unsafe {
            let mut key = ptr::null_mut();
            assert_eq!(
                RegCreateKeyExW(
                    self.sandbox,
                    wide(path).as_ptr(),
                    0,
                    ptr::null(),
                    REG_OPTION_VOLATILE,
                    KEY_SET_VALUE,
                    ptr::null(),
                    &mut key,
                    ptr::null_mut()
                ),
                ERROR_SUCCESS
            );
            let status = RegSetValueExW(
                key,
                wide(name).as_ptr(),
                0,
                kind,
                bytes.as_ptr(),
                bytes.len() as u32,
            );
            RegCloseKey(key);
            assert_eq!(status, ERROR_SUCCESS);
        }
    }

    pub fn write_string(&self, path: &str, name: &str, value: &str) {
        let bytes: Vec<_> = wide(value).iter().flat_map(|v| v.to_le_bytes()).collect();
        self.write(path, name, REG_SZ, &bytes);
    }

    pub fn string(&self, path: &str, name: &str) -> Option<String> {
        unsafe {
            let mut buffer = vec![0_u16; 32768];
            let mut size = (buffer.len() * 2) as u32;
            let status = RegGetValueW(
                self.sandbox,
                wide(path).as_ptr(),
                wide(name).as_ptr(),
                RRF_RT_REG_SZ,
                ptr::null_mut(),
                buffer.as_mut_ptr().cast(),
                &mut size,
            );
            if status == ERROR_FILE_NOT_FOUND {
                return None;
            }
            assert_eq!(status, ERROR_SUCCESS);
            buffer.truncate(size as usize / 2);
            Some(
                String::from_utf16_lossy(&buffer)
                    .trim_end_matches('\0')
                    .into(),
            )
        }
    }
}

impl Drop for CurrentUser {
    fn drop(&mut self) {
        unsafe {
            RegOverridePredefKey(HKEY_CURRENT_USER, ptr::null_mut());
            RegCloseKey(self.sandbox);
            RegDeleteTreeW(self.original, self.path.as_ptr());
            RegCloseKey(self.original);
        }
    }
}
