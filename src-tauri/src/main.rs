#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Some(code) = rpengine_manager_lib::elevation::run_helper_if_requested() {
        std::process::exit(code);
    }
    rpengine_manager_lib::run();
}
