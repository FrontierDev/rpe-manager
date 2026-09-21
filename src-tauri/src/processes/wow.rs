//! Centralized World of Warcraft process inspection and write-safety checks.

use std::fmt;

use serde::{Deserialize, Serialize};
use sysinfo::System;

const WOW_PROCESS_NAMES: [&str; 6] = [
    "wow.exe",
    "wow-64.exe",
    "wowt.exe",
    "wowt-64.exe",
    "wowb.exe",
    "wowb-64.exe",
];

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WowModificationSafetyState {
    pub is_wow_running: bool,
    pub matching_process_names: Vec<String>,
    pub can_modify_wow_files: bool,
}

impl WowModificationSafetyState {
    pub fn require_modification_allowed(&self) -> Result<(), WowModificationSafetyError> {
        if self.can_modify_wow_files {
            Ok(())
        } else {
            Err(WowModificationSafetyError::GameRunning {
                process_names: self.matching_process_names.clone(),
            })
        }
    }
}

/// Performs a fresh live process enumeration. Future file-changing commands
/// must call this through `require_wow_modification_allowed` immediately before
/// they modify `Interface/AddOns` or `WTF`.
pub fn inspect_wow_processes() -> WowModificationSafetyState {
    let system = System::new_all();
    safety_state_from_process_names(
        system
            .processes()
            .values()
            .map(|process| process.name().to_string_lossy().into_owned()),
    )
}

/// Rechecks the live state and returns an error that prevents a write when a
/// WoW client is running.
pub fn require_wow_modification_allowed(
) -> Result<WowModificationSafetyState, WowModificationSafetyError> {
    let state = inspect_wow_processes();
    state.require_modification_allowed()?;
    Ok(state)
}

pub fn is_wow_process_name(name: &str) -> bool {
    WOW_PROCESS_NAMES
        .iter()
        .any(|expected| name.eq_ignore_ascii_case(expected))
}

pub fn safety_state_from_process_names<I, S>(process_names: I) -> WowModificationSafetyState
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut matching_process_names: Vec<String> = process_names
        .into_iter()
        .map(|name| name.as_ref().to_owned())
        .filter(|name| is_wow_process_name(name))
        .collect();
    matching_process_names.sort_by_cached_key(|name| name.to_ascii_lowercase());
    matching_process_names.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    let is_wow_running = !matching_process_names.is_empty();

    WowModificationSafetyState {
        is_wow_running,
        matching_process_names,
        can_modify_wow_files: !is_wow_running,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WowModificationSafetyError {
    GameRunning { process_names: Vec<String> },
}

impl fmt::Display for WowModificationSafetyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GameRunning { .. } => formatter.write_str(
                "World of Warcraft is currently running. Close it before modifying RPEngine files or SavedVariables.",
            ),
        }
    }
}

impl std::error::Error for WowModificationSafetyError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_wow_process_allows_modifications() {
        let state = safety_state_from_process_names(["explorer.exe", "RPEngineManager.exe"]);

        assert!(!state.is_wow_running);
        assert!(state.can_modify_wow_files);
        assert!(state.require_modification_allowed().is_ok());
    }

    #[test]
    fn retail_client_prevents_modifications() {
        let state = safety_state_from_process_names(["Wow.exe"]);

        assert!(state.is_wow_running);
        assert!(!state.can_modify_wow_files);
        assert_eq!(state.matching_process_names, ["Wow.exe"]);
        assert!(matches!(
            state.require_modification_allowed(),
            Err(WowModificationSafetyError::GameRunning { .. })
        ));
    }

    #[test]
    fn ptr_and_beta_client_names_prevent_modifications() {
        let state = safety_state_from_process_names(["WowT-64.exe", "wowb.exe"]);

        assert!(state.is_wow_running);
        assert_eq!(state.matching_process_names, ["wowb.exe", "WowT-64.exe"]);
    }

    #[test]
    fn similarly_named_unrelated_processes_do_not_match() {
        let state = safety_state_from_process_names([
            "WowHelper.exe",
            "notwow.exe",
            "Wow.exe.bak",
            "WorldOfWarcraft.exe",
        ]);

        assert!(!state.is_wow_running);
        assert!(state.matching_process_names.is_empty());
    }
}
