fn main() {
    // The app's commands are allowed per window in capabilities/ (the page gets all of
    // them, the focus timer windows only `focus_hide`), not to every window by default.
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "status",
            "connect",
            "sign_out",
            "api",
            "outbox_status",
            "use_local",
            "open_data_folder",
            "set_local_password",
            "focus_hide",
            "download",
            "events_open",
            "events_close",
            "open_external",
        ]),
    ))
    .expect("tauri build");
}
