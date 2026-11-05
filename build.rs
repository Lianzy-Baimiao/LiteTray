fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set("ProductName", "LiteTray")
            .set(
                "FileDescription",
                "LiteTray — native Windows tray and calendar",
            )
            .set_manifest(include_str!("assets/app.manifest"))
            .compile_for("LiteTray")
            .expect("Windows resources could not be compiled");
    }
    println!("cargo:rerun-if-changed=assets/app.manifest");
}
