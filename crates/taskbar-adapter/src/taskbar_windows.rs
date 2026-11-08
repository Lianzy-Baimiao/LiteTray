//! Exact-class discovery also finds taskbars omitted by EnumThreadWindows on
//! the tested 25H2 desktop. Validate the complete desktop query before handing
//! only this process/thread's windows to any private host reader.
use windows_sys::Win32::{
    Foundation::{GetLastError, HWND, SetLastError},
    System::Threading::{GetCurrentProcessId, GetCurrentThreadId},
    UI::WindowsAndMessaging::{
        FindWindowExW, GA_PARENT, GA_ROOT, GetAncestor, GetClassNameW, GetDesktopWindow,
        GetWindowThreadProcessId,
    },
};

const CLASSES: [&str; 2] = ["Shell_TrayWnd", "Shell_SecondaryTrayWnd"];
const MAX_TASKBARS: usize = 64;

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct TaskbarWindow {
    pub(crate) handle: usize,
    pub(crate) secondary: bool,
}

struct Identity {
    class: String,
    root: usize,
    parent: usize,
    process_id: u32,
    thread_id: u32,
}

fn validate_identity(
    handle: usize,
    expected: &str,
    desktop: usize,
    identity: &Identity,
) -> Result<(), String> {
    if handle == 0
        || desktop == 0
        || identity.class != expected
        || identity.root != handle
        || identity.parent != desktop
        || identity.process_id == 0
        || identity.thread_id == 0
    {
        return Err("任务栏窗口在查询期间变化或不属于桌面顶层，稍后重新检测".into());
    }
    Ok(())
}

fn discover(
    process_id: u32,
    thread_id: u32,
    desktop: usize,
    mut next: impl FnMut(&str, usize) -> Result<usize, String>,
    mut identify: impl FnMut(usize) -> Result<Identity, String>,
) -> Result<Vec<TaskbarWindow>, String> {
    if process_id == 0 || thread_id == 0 || desktop == 0 {
        return Err("无法核对当前任务栏线程或桌面身份".into());
    }
    let mut seen = Vec::new();
    let mut windows = Vec::new();
    for expected in CLASSES {
        let mut previous = 0;
        loop {
            let handle = next(expected, previous)?;
            if handle == 0 {
                break;
            }
            // Count foreign windows too: filtering must not hide a cycle or
            // allow an unbounded desktop query on the Explorer thread.
            if seen.len() >= MAX_TASKBARS || seen.contains(&handle) {
                return Err("任务栏窗口查询超限或重复，稍后重新检测".into());
            }
            let identity = identify(handle)?;
            validate_identity(handle, expected, desktop, &identity)?;
            seen.push(handle);
            if identity.process_id == process_id && identity.thread_id == thread_id {
                windows.push(TaskbarWindow {
                    handle,
                    secondary: expected == CLASSES[1],
                });
            }
            previous = handle;
        }
    }
    Ok(windows)
}

fn read_identity(handle: usize) -> Result<Identity, String> {
    unsafe {
        let hwnd = handle as HWND;
        let mut class = [0u16; 128];
        let length = GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32);
        if length <= 0 {
            return Err("无法读取任务栏窗口类名，稍后重新检测".into());
        }
        let mut process_id = 0;
        let thread_id = GetWindowThreadProcessId(hwnd, &mut process_id);
        Ok(Identity {
            class: String::from_utf16_lossy(&class[..length as usize]),
            root: GetAncestor(hwnd, GA_ROOT) as usize,
            parent: GetAncestor(hwnd, GA_PARENT) as usize,
            process_id,
            thread_id,
        })
    }
}

pub(crate) fn current_thread() -> Result<Vec<TaskbarWindow>, String> {
    unsafe {
        discover(
            GetCurrentProcessId(),
            GetCurrentThreadId(),
            GetDesktopWindow() as usize,
            |expected, previous| {
                let class = crate::wide(expected);
                SetLastError(0);
                let handle = FindWindowExW(
                    std::ptr::null_mut(),
                    previous as HWND,
                    class.as_ptr(),
                    std::ptr::null(),
                );
                if handle.is_null() {
                    let error = GetLastError();
                    if error != 0 {
                        return Err(format!("任务栏窗口查询失败（{expected}，Win32 {error}）"));
                    }
                }
                Ok(handle as usize)
            },
            read_identity,
        )
    }
}

impl TaskbarWindow {
    pub(crate) fn validate_current(&self) -> Result<(), String> {
        let identity = read_identity(self.handle)?;
        unsafe {
            validate_identity(
                self.handle,
                CLASSES[usize::from(self.secondary)],
                GetDesktopWindow() as usize,
                &identity,
            )?;
            if identity.process_id != GetCurrentProcessId()
                || identity.thread_id != GetCurrentThreadId()
            {
                return Err("任务栏窗口进程或线程已变化，稍后重新检测".into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(handle: usize, secondary: bool, process_id: u32, thread_id: u32) -> Identity {
        Identity {
            class: CLASSES[usize::from(secondary)].into(),
            root: handle,
            parent: 1000,
            process_id,
            thread_id,
        }
    }

    #[test]
    fn discovers_all_primaries_and_secondaries_but_filters_both_owner_ids() {
        let mut queried = Vec::new();
        let windows = discover(
            10,
            20,
            1000,
            |class, previous| {
                queried.push((class.to_owned(), previous));
                Ok(match (class, previous) {
                    ("Shell_TrayWnd", 0) => 1,
                    ("Shell_TrayWnd", 1) => 2,
                    ("Shell_TrayWnd", 2) => 3,
                    ("Shell_TrayWnd", 3) => 4,
                    ("Shell_SecondaryTrayWnd", 0) => 5,
                    ("Shell_SecondaryTrayWnd", 5) => 6,
                    _ => 0,
                })
            },
            |handle| {
                let (process, thread) = match handle {
                    1 => (11, 20),
                    2 => (10, 21),
                    _ => (10, 20),
                };
                Ok(identity(handle, handle >= 5, process, thread))
            },
        )
        .unwrap();
        assert_eq!(
            windows,
            [
                TaskbarWindow {
                    handle: 3,
                    secondary: false
                },
                TaskbarWindow {
                    handle: 4,
                    secondary: false
                },
                TaskbarWindow {
                    handle: 5,
                    secondary: true
                },
                TaskbarWindow {
                    handle: 6,
                    secondary: true
                },
            ]
        );
        assert_eq!(queried.len(), 8);
        assert_eq!(queried.last(), Some(&(CLASSES[1].into(), 6)));
    }

    #[test]
    fn query_and_identity_errors_discard_previously_valid_candidates() {
        for fail_query in [false, true] {
            let result = discover(
                10,
                20,
                1000,
                |_, previous| match previous {
                    0 => Ok(1),
                    1 if fail_query => Err("query failed".into()),
                    _ => Ok(2),
                },
                |handle| {
                    if handle == 2 {
                        Err("window disappeared".into())
                    } else {
                        Ok(identity(handle, false, 10, 20))
                    }
                },
            );
            assert_eq!(
                result.unwrap_err(),
                if fail_query {
                    "query failed"
                } else {
                    "window disappeared"
                }
            );
        }
    }

    #[test]
    fn a_secondary_query_failure_does_not_consume_the_discovered_primary() {
        let mut consumed = 0;
        let result = discover(
            10,
            20,
            1000,
            |class, previous| {
                if class == CLASSES[1] {
                    Err("secondary query failed".into())
                } else {
                    Ok(if previous == 0 { 1 } else { 0 })
                }
            },
            |handle| Ok(identity(handle, false, 10, 20)),
        )
        .inspect(|windows| {
            consumed += windows.len();
        });
        assert_eq!(result.unwrap_err(), "secondary query failed");
        assert_eq!(consumed, 0);
    }

    #[test]
    fn rejects_changed_class_non_top_level_and_missing_ownership_even_if_foreign() {
        for invalid in 0..5 {
            let result = discover(
                10,
                20,
                1000,
                |_, previous| Ok(previous + 1),
                |handle| {
                    let mut value = identity(handle, false, 11, 21);
                    if handle == 2 {
                        match invalid {
                            0 => value.class = CLASSES[1].into(),
                            1 => value.root = 999,
                            2 => value.parent = 999,
                            3 => value.process_id = 0,
                            _ => value.thread_id = 0,
                        }
                    }
                    Ok(value)
                },
            );
            assert!(result.is_err());
        }
    }

    #[test]
    fn rejects_cycles_in_foreign_windows_and_duplicates_across_classes() {
        for across_classes in [false, true] {
            let mut calls = 0;
            let result = discover(
                10,
                20,
                1000,
                |_, previous| {
                    calls += 1;
                    Ok(if across_classes && previous == 1 {
                        0
                    } else {
                        1
                    })
                },
                |handle| Ok(identity(handle, false, 11, 21)),
            );
            assert!(result.is_err());
            assert_eq!(calls, if across_classes { 3 } else { 2 });
        }
    }

    #[test]
    fn applies_64_window_limit_across_classes_before_filtering_owners() {
        for count in [MAX_TASKBARS, MAX_TASKBARS + 1] {
            let result = discover(
                10,
                20,
                1000,
                |class, previous| {
                    Ok(if class == CLASSES[0] {
                        if previous == 0 { 1 } else { 0 }
                    } else {
                        let next = if previous == 0 { 2 } else { previous + 1 };
                        if next <= count { next } else { 0 }
                    })
                },
                |handle| Ok(identity(handle, handle != 1, 11, 21)),
            );
            if count == MAX_TASKBARS {
                assert!(result.unwrap().is_empty());
            } else {
                assert!(result.is_err());
            }
        }
    }

    #[test]
    fn an_empty_desktop_is_complete_but_missing_current_identity_is_an_error() {
        assert!(
            discover(10, 20, 1000, |_, _| Ok(0), |_| unreachable!())
                .unwrap()
                .is_empty()
        );
        for (process, thread, desktop) in [(0, 20, 1000), (10, 0, 1000), (10, 20, 0)] {
            assert!(
                discover(
                    process,
                    thread,
                    desktop,
                    |_, _| unreachable!(),
                    |_| unreachable!()
                )
                .is_err()
            );
        }
    }
}
