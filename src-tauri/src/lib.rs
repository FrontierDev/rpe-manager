pub mod commands;
pub mod configuration;
pub mod diagnostics;
pub mod discovery;
pub mod filesystem;
pub mod packages;
pub mod processes;
pub mod protocol;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::backups::list_manager_backups,
            commands::configuration::load_manager_configuration,
            commands::configuration::save_manager_configuration,
            commands::diagnostics::get_phase_one_diagnostics,
            commands::discovery::discover_wow_installations,
            commands::discovery::select_wow_installation,
            commands::local_discovery::discover_selected_wow_installation,
            commands::processes::get_wow_modification_safety_state,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run RPEngine Manager");
}
