//! Windows-only, single-purpose elevation bridge for addon replacement.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    addon_transaction::{replace_staged_addon_privileged, AddonTransactionReport},
    discovery::wow::validate_installation_path,
    release::{validate_staged_release, StagedRelease},
};

const HELPER_FLAG: &str = "--rpengine-addon-helper";

#[derive(Debug, Serialize, Deserialize)]
struct HelperResult {
    ok: bool,
    version: Option<String>,
    message: Option<String>,
}

pub fn run_helper_if_requested() -> bool {
    let args: Vec<_> = env::args_os().collect();
    if args.get(1).and_then(|value| value.to_str()) != Some(HELPER_FLAG) {
        return false;
    }
    let result_path = args.get(4).map(PathBuf::from);
    let result = match (args.get(2), args.get(3), result_path.as_ref()) {
        (Some(installation), Some(stage), Some(_)) => {
            helper_replace(Path::new(installation), Path::new(stage))
        }
        _ => Err("Invalid elevated helper request.".to_owned()),
    };
    if let Some(path) = result_path {
        let payload = match result {
            Ok(report) => HelperResult {
                ok: true,
                version: Some(report.version),
                message: None,
            },
            Err(message) => HelperResult {
                ok: false,
                version: None,
                message: Some(message),
            },
        };
        let _ = fs::write(path, serde_json::to_vec(&payload).unwrap_or_default());
    }
    true
}

fn helper_replace(installation: &Path, stage: &Path) -> Result<AddonTransactionReport, String> {
    validate_installation_path(installation)
        .map_err(|_| "Elevated helper rejected a target outside a WoW installation.".to_owned())?;
    // Scope guard: only a Manager-created operation directory may be used.
    if !stage
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with("rpengine-stage-"))
        || stage
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            != Some("release-staging")
    {
        return Err("Elevated helper rejected an unvalidated staging directory.".to_owned());
    }
    let staged = StagedRelease {
        version: version_from_stage(stage)?,
        directory: stage.to_path_buf(),
    };
    validate_staged_release(&staged)
        .map_err(|e| format!("Elevated helper rejected staged package: {e}"))?;
    replace_staged_addon_privileged(installation, &staged).map_err(|e| e.to_string())
}

fn version_from_stage(stage: &Path) -> Result<String, String> {
    let toc = fs::read_to_string(stage.join("RPEngine2/RPEngine2.toc"))
        .map_err(|_| "Elevated helper could not read staged metadata.".to_owned())?;
    toc.lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("## Version:")
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
        })
        .ok_or_else(|| "Elevated helper rejected staged package metadata.".to_owned())
}

#[cfg(windows)]
pub fn replace_elevated(
    installation: &Path,
    staged: &StagedRelease,
) -> Result<AddonTransactionReport, String> {
    let executable = env::current_exe().map_err(|e| e.to_string())?;
    let result = staged.directory.join("helper-result.json");
    let _ = fs::remove_file(&result);
    // `powershell -Command` appends trailing arguments to the command text,
    // which breaks paths with spaces (the normal Program Files case).
    // Start-Process also flattens an ArgumentList array, so supply one
    // Windows-quoted command line rather than raw path elements.
    let helper_arguments = [
        HELPER_FLAG.to_owned(),
        installation.to_string_lossy().into_owned(),
        staged.directory.to_string_lossy().into_owned(),
        result.to_string_lossy().into_owned(),
    ]
    .into_iter()
    .map(|argument| quote_windows_argument(&argument))
    .collect::<Vec<_>>()
    .join(" ");
    runas_and_wait(executable.as_os_str(), &helper_arguments)?;
    let payload: HelperResult = fs::read(&result)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .ok_or_else(|| "Elevated helper did not return a result.".to_owned())?;
    let _ = fs::remove_file(result);
    if payload.ok {
        Ok(AddonTransactionReport {
            version: payload
                .version
                .ok_or_else(|| "Elevated helper returned no version.".to_owned())?,
        })
    } else {
        Err(payload
            .message
            .unwrap_or_else(|| "Elevated addon replacement failed.".to_owned()))
    }
}

#[cfg(windows)]
fn runas_and_wait(executable: &std::ffi::OsStr, arguments: &str) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE},
        UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW},
    };

    let wide = |value: &std::ffi::OsStr| value.encode_wide().chain(Some(0)).collect::<Vec<_>>();
    let verb = wide(std::ffi::OsStr::new("runas"));
    let executable = wide(executable);
    let arguments = wide(std::ffi::OsStr::new(arguments));
    let mut request: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    request.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    request.fMask = SEE_MASK_NOCLOSEPROCESS;
    request.lpVerb = verb.as_ptr();
    request.lpFile = executable.as_ptr();
    request.lpParameters = arguments.as_ptr();
    request.nShow = 0;
    if unsafe { ShellExecuteExW(&mut request) } == 0 {
        let error = std::io::Error::last_os_error();
        return Err(if error.raw_os_error() == Some(1223) {
            "Administrator permission was declined.".to_owned()
        } else {
            format!("Could not request administrator permission: {error}")
        });
    }
    if request.hProcess.is_null() {
        return Err("Elevated helper did not provide a process handle.".to_owned());
    }
    unsafe { WaitForSingleObject(request.hProcess, INFINITE) };
    let mut exit_code = 0;
    let got_exit_code = unsafe { GetExitCodeProcess(request.hProcess, &mut exit_code) } != 0;
    unsafe { CloseHandle(request.hProcess) };
    if !got_exit_code || exit_code != 0 {
        return Err("Elevated helper exited before completing the addon replacement.".to_owned());
    }
    Ok(())
}

/// Quotes one argument using the Windows `CommandLineToArgvW` rules.
fn quote_windows_argument(value: &str) -> String {
    let mut quoted = String::from("\"");
    let mut slashes = 0;
    for character in value.chars() {
        match character {
            '\\' => slashes += 1,
            '\"' => {
                quoted.push_str(&"\\".repeat(slashes * 2 + 1));
                quoted.push('\"');
                slashes = 0;
            }
            _ => {
                quoted.push_str(&"\\".repeat(slashes));
                quoted.push(character);
                slashes = 0;
            }
        }
    }
    quoted.push_str(&"\\".repeat(slashes * 2));
    quoted.push('\"');
    quoted
}

#[cfg(not(windows))]
pub fn replace_elevated(_: &Path, _: &StagedRelease) -> Result<AddonTransactionReport, String> {
    Err("Administrator elevation is only available on Windows.".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn helper_rejects_arbitrary_stage_path() {
        assert!(helper_replace(Path::new("C:/not-wow"), Path::new("C:/arbitrary")).is_err());
    }

    #[test]
    fn helper_rejects_arbitrary_target_path() {
        let stage = Path::new("C:/release-staging/rpengine-stage-test");
        assert!(helper_replace(Path::new("C:/arbitrary"), stage).is_err());
    }

    #[test]
    fn quotes_windows_paths_without_losing_spaces_or_trailing_slashes() {
        assert_eq!(
            quote_windows_argument("C:\\Program Files (x86)\\WoW\\"),
            "\"C:\\Program Files (x86)\\WoW\\\\\""
        );
        assert_eq!(quote_windows_argument("a\"b"), "\"a\\\"b\"");
    }
}
