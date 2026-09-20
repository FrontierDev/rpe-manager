pub mod commands;
pub mod diagnostics;
pub mod discovery;
pub mod filesystem;
pub mod packages;
pub mod processes;
pub mod protocol;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("failed to run RPEngine Manager");
}
