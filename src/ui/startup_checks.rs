use super::*;
#[path = "../../tests/support/windows_registry.rs"]
mod registry;

const RUN: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";

struct Profile {
    directory: std::path::PathBuf,
    parent: std::path::PathBuf,
    previous: Option<std::ffi::OsString>,
}
impl Drop for Profile {
    fn drop(&mut self) {
        unsafe {
            if let Some(previous) = &self.previous {
                std::env::set_var("LITETRAY_DATA_DIR", previous);
            } else {
                std::env::remove_var("LITETRAY_DATA_DIR");
            }
        }
        if let Ok(resolved) = std::fs::canonicalize(&self.directory)
            && resolved == self.directory
            && resolved.parent() == Some(self.parent.as_path())
        {
            let _ = std::fs::remove_dir_all(resolved);
        }
    }
}

#[test]
fn saving_enabled_startup_repairs_a_stale_or_missing_run_entry() {
    visual_checks::on_isolated_desktop(|| unsafe {
        let registry = registry::CurrentUser::new();
        let parent = std::fs::canonicalize(std::env::temp_dir()).unwrap();
        let directory = parent.join(format!(
            "litetray-startup-save-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap()
        ));
        std::fs::create_dir(&directory).unwrap();
        let profile = Profile {
            directory,
            parent,
            previous: std::env::var_os("LITETRAY_DATA_DIR"),
        };
        std::env::set_var("LITETRAY_DATA_DIR", &profile.directory);
        let mut app =
            visual_checks::app_for_check(false, 96, &profile.directory.join("agenda.json"));
        app.saved.start_with_windows = true;
        app.draft.start_with_windows = true;
        app.page = 4;
        app.render();
        assert!(app.checked(502));

        let expected = format!(
            "\"{}\" --background",
            std::env::current_exe().unwrap().display()
        );
        registry.write_string(
            RUN,
            "LiteTray",
            "\"C:\\old-portable\\LiteTray.exe\" --background",
        );
        app.save().unwrap();
        assert_eq!(
            registry.string(RUN, "LiteTray"),
            Some(expected.clone()),
            "saving an already enabled preference must repair the old executable path"
        );
        startup::set_enabled(false).unwrap();
        app.save().unwrap();
        assert_eq!(registry.string(RUN, "LiteTray"), Some(expected));
        assert!(
            config::load(&profile.directory.join("config.json"))
                .unwrap()
                .start_with_windows
        );

        DestroyWindow(app.hwnd);
        app.release_resources();
    });
}
