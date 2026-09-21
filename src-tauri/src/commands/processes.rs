//! Commands that expose the centralized WoW process safety state.

use crate::processes::wow::{inspect_wow_processes, WowModificationSafetyState};

#[tauri::command]
pub fn get_wow_modification_safety_state() -> WowModificationSafetyState {
    inspect_wow_processes()
}
