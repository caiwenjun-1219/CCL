const COMMANDS: &[&str] = &["check_storage_permission", "request_storage_permission"];

fn main() {
    tauri_plugin::Builder::new(COMMANDS)
        .android_path("android")
        .try_build()
        .expect("failed to build ccl-android plugin");
}
