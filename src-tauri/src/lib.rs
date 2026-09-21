pub mod addon_transaction;
pub mod catalogue;
pub mod commands;
pub mod configuration;
pub mod diagnostics;
pub mod discovery;
pub mod elevation;
pub mod filesystem;
pub mod packages;
pub mod processes;
pub mod protocol;
pub mod release;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::backups::list_manager_backups,
            commands::catalogue::get_catalogue_packages,
            commands::catalogue::get_catalogue_package,
            commands::catalogue::get_catalogue_publishers,
            commands::configuration::load_manager_configuration,
            commands::configuration::save_manager_configuration,
            commands::diagnostics::get_phase_one_diagnostics,
            commands::discovery::discover_wow_installations,
            commands::discovery::select_wow_installation,
            commands::local_discovery::discover_selected_wow_installation,
            commands::operations::queue_install_dataset,
            commands::operations::queue_catalogue_package_install,
            commands::operations::queue_install_ruleset,
            commands::operations::queue_remove_dataset,
            commands::processes::get_wow_modification_safety_state,
            commands::protocol_state::get_selected_protocol_state,
            commands::protocol_state::reconcile_selected_protocol_request,
            commands::release::install_latest_rpengine,
            commands::release::get_rpengine_update_state,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run RPEngine Manager");
}
