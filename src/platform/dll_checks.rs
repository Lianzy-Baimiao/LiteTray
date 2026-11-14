use super::*;
use std::{os::windows::process::CommandExt, process::Command};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

const CHILD_DIRECTORY: &str = "LITETRAY_DLL_TEST_DIRECTORY";

#[test]
fn replacement_isolated_in_child_process() {
    let directory = std::env::temp_dir().join(format!(
        "litetray-dll-tests-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    let result = std::panic::catch_unwind(|| {
        for value in 1..=2 {
            let source = directory.join(format!("fixture{value}.rs"));
            std::fs::write(
                &source,
                format!(
                    "#[unsafe(no_mangle)] pub extern \"system\" fn LiteTrayFixtureValue() -> u32 {{ {value} }}"
                ),
            )
            .unwrap();
            let mut compiler = Command::new("rustc");
            compiler
                .arg(&source)
                .args(["--edition=2024", "--crate-type=cdylib", "-C", "panic=abort"])
                .arg("-o")
                .arg(directory.join(format!("fixture{value}.dll")))
                .creation_flags(0x0800_0000);
            if let Some(linker) = std::env::var_os("CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER") {
                compiler
                    .arg("-C")
                    .arg(format!("linker={}", linker.to_string_lossy()));
            }
            let output = compiler.output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "platform::dll::tests::replacement_child",
                "--nocapture",
            ])
            .env(CHILD_DIRECTORY, &directory)
            .creation_flags(0x0800_0000)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        println!("{}", String::from_utf8_lossy(&output.stdout));
    });
    // Only remove the newly created, resolved child of the intended temp root.
    let resolved = std::fs::canonicalize(&directory).unwrap();
    assert_eq!(
        resolved.parent().unwrap(),
        std::fs::canonicalize(std::env::temp_dir()).unwrap()
    );
    assert!(
        resolved
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("litetray-dll-tests-")
    );
    std::fs::remove_dir_all(resolved).unwrap();
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
}

fn value(module: HMODULE) -> u32 {
    unsafe {
        let entry = GetProcAddress(module, c"LiteTrayFixtureValue".as_ptr().cast()).unwrap();
        let function: unsafe extern "system" fn() -> u32 = std::mem::transmute(entry);
        function()
    }
}

fn deny_directory_creation(directory: &Path, deny: bool) {
    let mut command = Command::new("icacls.exe");
    command.arg(directory);
    if deny {
        command.args(["/deny", "*S-1-1-0:(WD,AD)"]);
    } else {
        command.args(["/remove:d", "*S-1-1-0"]);
    }
    let output = command.creation_flags(0x0800_0000).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
struct ReadOnlyDirectory(PathBuf);
impl Drop for ReadOnlyDirectory {
    fn drop(&mut self) {
        deny_directory_creation(&self.0, false);
    }
}

#[test]
fn replacement_child() {
    let Some(directory) = std::env::var_os(CHILD_DIRECTORY).map(PathBuf::from) else {
        return;
    };
    let first = directory.join("fixture1.dll");
    let second = directory.join("fixture2.dll");

    // Reproduce the actual Windows loader failure, isolated from Explorer.
    let legacy = directory.join("legacy.dll");
    std::fs::copy(&first, &legacy).unwrap();
    let old = unsafe { LoadLibraryW(wide_path(&legacy).as_ptr()) };
    assert!(!old.is_null());
    assert_eq!(value(old), 1);
    std::fs::rename(&legacy, directory.join("legacy.retained.dll")).unwrap();
    std::fs::copy(&second, &legacy).unwrap();
    let stale = unsafe { LoadLibraryW(wide_path(&legacy).as_ptr()) };
    assert_eq!(
        old, stale,
        "baseline must reproduce reuse of the renamed image"
    );
    assert_eq!(
        value(stale),
        1,
        "fixed path returned the old exported function"
    );
    unsafe {
        FreeLibrary(stale);
        FreeLibrary(old);
    }

    let source = directory.join("litetray_taskbar_adapter.dll");
    std::fs::copy(&first, &source).unwrap();
    let worker_directory = directory.clone();
    let prepared = std::thread::spawn(move || prepare(&worker_directory, Kind::Taskbar).unwrap())
        .join()
        .unwrap();
    assert_eq!(
        prepared.sha256,
        format!("{:x}", Sha256::digest(std::fs::read(&first).unwrap()))
    );
    std::fs::copy(&second, &source).unwrap();
    let current = prepared.load().unwrap();
    assert_eq!(
        value(current.handle()),
        1,
        "worker-verified bytes and their identity survive source replacement"
    );
    let updated = load_from(&directory, Kind::Taskbar).unwrap();
    assert_ne!(current.handle(), updated.handle());
    assert_eq!(value(current.handle()), 1);
    assert_eq!(value(updated.handle()), 2);
    let prepared = prepare(&directory, Kind::Taskbar).unwrap();
    let path = &prepared.path;
    assert_eq!(
        prepared.sha256,
        format!("{:x}", Sha256::digest(std::fs::read(&second).unwrap()))
    );
    assert_eq!(
        path.parent().unwrap(),
        directory,
        "same-installation boundary is preserved"
    );
    assert!(OpenOptions::new().write(true).open(path).is_err());
    assert!(std::fs::rename(path, directory.join("renamed.dll")).is_err());
    drop(prepared);
    drop(updated);
    drop(current);

    // A packaged, pre-existing image loads with directory creation denied.
    deny_directory_creation(&directory, true);
    {
        let _restore = ReadOnlyDirectory(directory.clone());
        assert!(File::create(directory.join("must-not-write.tmp")).is_err());
        let packaged = load_from(&directory, Kind::Taskbar).unwrap();
        assert_eq!(value(packaged.handle()), 2);
    }

    // Publish races reuse complete files and cannot overwrite an occupied name.
    let concurrent = directory.join("concurrent");
    std::fs::create_dir(&concurrent).unwrap();
    std::fs::copy(&second, concurrent.join("litetray_popup_hook.dll")).unwrap();
    let gate = std::sync::Arc::new(std::sync::Barrier::new(8));
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let directory = concurrent.clone();
            let gate = gate.clone();
            std::thread::spawn(move || {
                gate.wait();
                prepare(&directory, Kind::Popup).unwrap().path
            })
        })
        .collect();
    let paths: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    assert!(paths.iter().all(|p| p == &paths[0]));
    assert_eq!(std::fs::read_dir(&concurrent).unwrap().count(), 2);
    std::fs::write(&paths[0], b"damaged cached image").unwrap();
    assert!(prepare(&concurrent, Kind::Popup).is_err());
    assert_eq!(std::fs::read(&paths[0]).unwrap(), b"damaged cached image");
    std::fs::remove_file(&paths[0]).unwrap();
    std::fs::create_dir(&paths[0]).unwrap();
    assert!(prepare(&concurrent, Kind::Popup).is_err());
    std::fs::write(concurrent.join("litetray_popup_hook.dll"), b"invalid PE").unwrap();
    assert!(prepare(&concurrent, Kind::Popup).is_err());
    println!(
        "DLL loader: stale fixed-path image reproduced; distinct content versions, export values, file locking, read-only directory, eight publishers, corruption and non-file rejection passed"
    );
}
