//! Narrow Windows helper contract for the final RPEngine directory replacement.

use std::{
    env, fs, io,
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    addon_transaction::{replace_staged_addon_privileged, AddonTransactionReport},
    discovery::wow::validate_installation_path,
    release::{validate_staged_release, StagedRelease},
};

const HELPER_FLAG: &str = "--rpengine-addon-helper";
const DESCRIPTOR_SCHEMA: u32 = 1;
const RESULT_SCHEMA: u32 = 1;
const ADDON_NAME: &str = "RPEngine2";
#[cfg(windows)]
const MANAGER_IDENTIFIER: &str = "net.esarus.rpengine-manager";

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OperationDescriptor {
    pub schema_version: u32,
    pub operation_id: String,
    pub installation: PathBuf,
    pub addon_destination: PathBuf,
    pub staged_directory: PathBuf,
    pub target_version: String,
    pub package_identity: String,
    pub operation_directory: PathBuf,
    pub request_file: PathBuf,
    pub result_file: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HelperResult {
    schema_version: u32,
    operation_id: String,
    ok: bool,
    version: Option<String>,
    stage: Option<String>,
    message: Option<String>,
}

#[derive(Debug)]
struct HelperFailure {
    stage: &'static str,
    message: String,
}

impl HelperFailure {
    fn new(stage: &'static str, message: impl Into<String>) -> Self {
        Self {
            stage,
            message: message.into(),
        }
    }
}

pub fn operation_id() -> String {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{}-{nonce}", std::process::id())
}

/// The GUI calls this before its Tauri runtime starts. `Some(code)` means the
/// process was started as the helper and must exit with that exact code.
pub fn run_helper_if_requested() -> Option<i32> {
    let args: Vec<_> = env::args_os().collect();
    if args.get(1).and_then(|value| value.to_str()) != Some(HELPER_FLAG) {
        return None;
    }
    if args.len() != 4 {
        return Some(10);
    }
    let Some(operation_id) = args.get(2).and_then(|value| value.to_str()) else {
        return Some(10);
    };
    Some(run_helper_request(operation_id, Path::new(&args[3])))
}

/// Testable CLI contract: success is possible only after writing a structured
/// result. The result path is fixed beside the supplied descriptor.
fn run_helper_request(operation_id: &str, request_file: &Path) -> i32 {
    let manager_root = match manager_staging_root() {
        Ok(root) => root,
        Err(_) => return 10,
    };
    run_helper_request_under_root(operation_id, request_file, &manager_root)
}

fn run_helper_request_under_root(
    operation_id: &str,
    request_file: &Path,
    manager_root: &Path,
) -> i32 {
    if validate_request_location(operation_id, request_file, manager_root).is_err() {
        return 10;
    }
    let result_file = request_file
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("result.json");
    let outcome = read_and_validate_operation(operation_id, request_file, manager_root).and_then(
        |(descriptor, staged)| {
            replace_staged_addon_privileged(&descriptor.installation, &staged)
                .map_err(|error| HelperFailure::new(transaction_stage(&error), error.to_string()))
        },
    );

    let result = match outcome {
        Ok(report) => HelperResult {
            schema_version: RESULT_SCHEMA,
            operation_id: operation_id.to_owned(),
            ok: true,
            version: Some(report.version),
            stage: None,
            message: None,
        },
        Err(error) => HelperResult {
            schema_version: RESULT_SCHEMA,
            operation_id: operation_id.to_owned(),
            ok: false,
            version: None,
            stage: Some(error.stage.to_owned()),
            message: Some(error.message),
        },
    };
    match write_result(&result_file, &result) {
        Ok(()) if result.ok => 0,
        Ok(()) => 20,
        Err(_) => 40,
    }
}

fn transaction_stage(error: &crate::addon_transaction::AddonTransactionError) -> &'static str {
    use crate::addon_transaction::AddonTransactionError as E;
    match error {
        E::GameRunning(_) => "transaction.game_running",
        E::StagedRelease(_) => "stage.validation",
        E::InvalidInstallation(_) => "request.installation_validation",
        E::TargetNotWritable { .. } => "transaction.destination_access",
        E::CopyStaged { .. } => "transaction.stage_read",
        E::DisplaceExisting { .. } => "transaction.displacement",
        E::ActivateNew { .. } => "transaction.activation",
        E::BackupCleanup { .. } => "transaction.backup_cleanup",
        E::Verification(_) => "transaction.post_install_verification",
        E::Rollback { .. } => "transaction.rollback",
    }
}

fn write_result(path: &Path, result: &HelperResult) -> io::Result<()> {
    let bytes = serde_json::to_vec(result).map_err(io::Error::other)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    use io::Write;
    file.write_all(&bytes)?;
    file.sync_all()
}

fn write_operation_request(path: &Path, descriptor: &OperationDescriptor) -> io::Result<()> {
    let bytes = serde_json::to_vec_pretty(descriptor).map_err(io::Error::other)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    use io::Write;
    file.write_all(&bytes)?;
    file.sync_all()
}

fn read_and_validate_operation(
    command_operation_id: &str,
    request_file: &Path,
    manager_root: &Path,
) -> Result<(OperationDescriptor, StagedRelease), HelperFailure> {
    let bytes = fs::read(request_file).map_err(|error| {
        HelperFailure::new(
            "request.read",
            format!("Could not read operation request: {error}"),
        )
    })?;
    let descriptor: OperationDescriptor = serde_json::from_slice(&bytes).map_err(|error| {
        HelperFailure::new(
            "request.parse",
            format!("Operation request is malformed: {error}"),
        )
    })?;
    validate_operation_descriptor(
        &descriptor,
        command_operation_id,
        request_file,
        manager_root,
    )?;
    let staged = StagedRelease {
        version: descriptor.target_version.clone(),
        directory: descriptor.staged_directory.clone(),
    };
    validate_staged_release(&staged).map_err(|error| {
        HelperFailure::new(
            "stage.validation",
            format!("Could not validate staged release: {error}"),
        )
    })?;
    let identity = staged_package_identity(&staged.directory).map_err(|error| {
        HelperFailure::new(
            "stage.read",
            format!("Could not read staged package identity: {error}"),
        )
    })?;
    if identity != descriptor.package_identity {
        return Err(HelperFailure::new(
            "stage.identity",
            "Staged package identity does not match the operation request.",
        ));
    }
    Ok((descriptor, staged))
}

fn validate_request_location(
    operation_id: &str,
    request_file: &Path,
    manager_root: &Path,
) -> Result<(), HelperFailure> {
    let operation_directory = request_file.parent().ok_or_else(|| {
        HelperFailure::new(
            "request.path_validation",
            "Operation request has no parent directory.",
        )
    })?;
    let canonical_root = manager_root.canonicalize().map_err(|error| {
        HelperFailure::new(
            "request.storage_validation",
            format!("Could not resolve Manager staging root: {error}"),
        )
    })?;
    let canonical_operation = operation_directory.canonicalize().map_err(|error| {
        HelperFailure::new(
            "request.storage_validation",
            format!("Could not resolve operation directory: {error}"),
        )
    })?;
    if canonical_operation.parent() != Some(canonical_root.as_path())
        || canonical_operation
            .file_name()
            .and_then(|part| part.to_str())
            != Some(format!("rpengine-op-{operation_id}").as_str())
        || request_file.file_name().and_then(|part| part.to_str()) != Some("request.json")
    {
        return Err(HelperFailure::new(
            "request.storage_validation",
            "Helper request is outside Manager-owned release staging.",
        ));
    }
    Ok(())
}

/// Pure contract checks plus filesystem canonicalization. Kept public for
/// deterministic descriptor tests and shared by the actual helper entry point.
fn validate_operation_descriptor(
    descriptor: &OperationDescriptor,
    command_operation_id: &str,
    request_file: &Path,
    manager_root: &Path,
) -> Result<(), HelperFailure> {
    if descriptor.schema_version != DESCRIPTOR_SCHEMA {
        return Err(HelperFailure::new(
            "request.schema",
            "Unsupported operation request schema.",
        ));
    }
    if descriptor.operation_id != command_operation_id {
        return Err(HelperFailure::new(
            "request.identity",
            "Operation ID does not match the helper command.",
        ));
    }
    if descriptor.operation_id.is_empty()
        || !descriptor
            .operation_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err(HelperFailure::new(
            "request.identity",
            "Operation ID is invalid.",
        ));
    }

    let operation_root = descriptor
        .operation_directory
        .canonicalize()
        .map_err(|error| {
            HelperFailure::new(
                "request.storage_validation",
                format!("Could not resolve operation directory: {error}"),
            )
        })?;
    let manager_root = manager_root.canonicalize().map_err(|error| {
        HelperFailure::new(
            "request.storage_validation",
            format!("Could not resolve Manager staging root: {error}"),
        )
    })?;
    if operation_root.parent() != Some(manager_root.as_path())
        || operation_root.file_name().and_then(|part| part.to_str())
            != Some(format!("rpengine-op-{}", descriptor.operation_id).as_str())
    {
        return Err(HelperFailure::new(
            "request.storage_validation",
            "Operation directory is outside Manager-owned release staging.",
        ));
    }

    let canonical_request = request_file.canonicalize().map_err(|error| {
        HelperFailure::new(
            "request.path_validation",
            format!("Could not resolve request file: {error}"),
        )
    })?;
    let expected_request = operation_root.join("request.json");
    if canonical_request != expected_request || descriptor.request_file != expected_request {
        return Err(HelperFailure::new(
            "request.path_validation",
            "Request file path does not match the operation directory.",
        ));
    }
    let expected_result = operation_root.join("result.json");
    if descriptor.result_file != expected_result {
        return Err(HelperFailure::new(
            "request.path_validation",
            "Result file path does not match the operation directory.",
        ));
    }

    validate_installation_path(&descriptor.installation).map_err(|error| {
        HelperFailure::new(
            "request.installation_validation",
            format!("Selected directory is not a valid WoW installation: {error}"),
        )
    })?;
    let installation = descriptor.installation.canonicalize().map_err(|error| {
        HelperFailure::new(
            "request.installation_validation",
            format!("Could not resolve WoW installation: {error}"),
        )
    })?;
    let expected_destination = installation
        .join("Interface")
        .join("AddOns")
        .join(ADDON_NAME);
    if normalize_for_compare(&descriptor.addon_destination)
        != normalize_for_compare(&expected_destination)
    {
        return Err(HelperFailure::new(
            "request.destination_validation",
            "Addon destination is outside the selected installation.",
        ));
    }
    validate_no_redirected_existing_parent(&installation)?;

    let canonical_stage = descriptor
        .staged_directory
        .canonicalize()
        .map_err(|error| {
            HelperFailure::new(
                "stage.read",
                format!("Could not resolve staged directory: {error}"),
            )
        })?;
    let expected_stage_parent = operation_root.join("staged");
    if !canonical_stage.starts_with(&expected_stage_parent)
        || canonical_stage == expected_stage_parent
    {
        return Err(HelperFailure::new(
            "request.stage_validation",
            "Staged directory is outside this Manager operation.",
        ));
    }
    if descriptor.staged_directory != canonical_stage {
        return Err(HelperFailure::new(
            "request.stage_validation",
            "Staged directory path is not canonical.",
        ));
    }
    if crate::release::RpeVersion::parse(&descriptor.target_version).is_err() {
        return Err(HelperFailure::new(
            "request.version_validation",
            "Requested release version is invalid.",
        ));
    }
    Ok(())
}

fn normalize_for_compare(path: &Path) -> PathBuf {
    path.components()
        .filter(|component| !matches!(component, Component::CurDir))
        .collect()
}

fn validate_no_redirected_existing_parent(installation: &Path) -> Result<(), HelperFailure> {
    let expected_parent = installation.join("Interface").join("AddOns");
    let mut existing = expected_parent.as_path();
    while !existing.exists() {
        existing = existing.parent().ok_or_else(|| {
            HelperFailure::new(
                "request.destination_validation",
                "Addon destination has no existing parent.",
            )
        })?;
    }
    let canonical_existing = existing.canonicalize().map_err(|error| {
        HelperFailure::new(
            "request.destination_validation",
            format!("Could not resolve addon destination parent: {error}"),
        )
    })?;
    let suffix = expected_parent
        .strip_prefix(existing)
        .map_err(|error| HelperFailure::new("request.destination_validation", error.to_string()))?;
    let resolved_parent = canonical_existing.join(suffix);
    if normalize_for_compare(&resolved_parent) != normalize_for_compare(&expected_parent) {
        return Err(HelperFailure::new(
            "request.destination_validation",
            "Addon destination parent resolves outside the selected installation.",
        ));
    }
    Ok(())
}

fn manager_staging_root() -> Result<PathBuf, String> {
    #[cfg(windows)]
    {
        let local = env::var_os("LOCALAPPDATA")
            .ok_or_else(|| "LOCALAPPDATA is unavailable in the elevated process.".to_owned())?;
        Ok(PathBuf::from(local)
            .join(MANAGER_IDENTIFIER)
            .join("release-staging"))
    }
    #[cfg(not(windows))]
    {
        Err("The addon helper is supported only on Windows.".to_owned())
    }
}

pub fn staged_package_identity(directory: &Path) -> io::Result<String> {
    fn collect_files(
        root: &Path,
        directory: &Path,
        files: &mut Vec<(PathBuf, bool)>,
    ) -> io::Result<()> {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            let relative = path
                .strip_prefix(root)
                .map_err(io::Error::other)?
                .to_path_buf();
            if metadata.file_type().is_symlink() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "staged package contains a symbolic link",
                ));
            }
            if metadata.is_dir() {
                files.push((relative, true));
                collect_files(root, &path, files)?;
            } else if metadata.is_file() {
                files.push((relative, false));
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "staged package contains a non-file entry",
                ));
            }
        }
        Ok(())
    }

    let mut entries = Vec::new();
    collect_files(directory, directory, &mut entries)?;
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    let mut digest = Sha256::new();
    for (relative, is_directory) in entries {
        let name = relative.to_string_lossy().replace('\\', "/");
        digest.update((name.len() as u64).to_le_bytes());
        digest.update(name.as_bytes());
        digest.update([u8::from(is_directory)]);
        if !is_directory {
            let contents = fs::read(directory.join(relative))?;
            digest.update((contents.len() as u64).to_le_bytes());
            digest.update(contents);
        }
    }
    Ok(format!("sha256:{:x}", digest.finalize()))
}

#[cfg(windows)]
fn make_operation_descriptor(
    installation: &Path,
    staged: &StagedRelease,
) -> Result<(OperationDescriptor, String), String> {
    let operation_directory = staged
        .directory
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| "Staged release is not inside a Manager operation directory.".to_owned())?
        .canonicalize()
        .map_err(|error| format!("Could not resolve operation directory: {error}"))?;
    let operation_id = operation_directory
        .file_name()
        .and_then(|part| part.to_str())
        .and_then(|part| part.strip_prefix("rpengine-op-"))
        .ok_or_else(|| "Staged release is not inside a Manager operation directory.".to_owned())?
        .to_owned();
    let installation = installation
        .canonicalize()
        .map_err(|error| format!("Could not resolve selected WoW installation: {error}"))?;
    let staged_directory = staged
        .directory
        .canonicalize()
        .map_err(|error| format!("Could not resolve staged release: {error}"))?;
    let descriptor = OperationDescriptor {
        schema_version: DESCRIPTOR_SCHEMA,
        operation_id: operation_id.clone(),
        addon_destination: installation.join("Interface/AddOns/RPEngine2"),
        installation,
        staged_directory,
        target_version: staged.version.clone(),
        package_identity: staged_package_identity(&staged.directory)
            .map_err(|error| format!("Could not identify staged package: {error}"))?,
        request_file: operation_directory.join("request.json"),
        result_file: operation_directory.join("result.json"),
        operation_directory,
    };
    Ok((descriptor, operation_id))
}

#[cfg(windows)]
pub fn process_is_elevated() -> bool {
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY},
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };

    let mut token = std::ptr::null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return false;
    }
    let mut elevation: TOKEN_ELEVATION = unsafe { std::mem::zeroed() };
    let mut returned = 0;
    let success = unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            &mut elevation as *mut _ as *mut _,
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        )
    } != 0;
    unsafe { CloseHandle(token) };
    success && elevation.TokenIsElevated != 0
}

#[cfg(not(windows))]
pub fn process_is_elevated() -> bool {
    false
}

#[cfg(windows)]
pub fn replace_elevated(
    _installation: &Path,
    staged: &StagedRelease,
) -> Result<AddonTransactionReport, String> {
    let (descriptor, operation_id) = make_operation_descriptor(_installation, staged)?;
    write_operation_request(&descriptor.request_file, &descriptor)
        .map_err(|error| format!("Could not write helper operation request: {error}"))?;
    let executable = env::current_exe()
        .map_err(|error| format!("Could not locate Manager executable: {error}"))?;
    let helper_arguments = build_helper_arguments(&operation_id, &descriptor.request_file);
    let exit_code = runas_and_wait(executable.as_os_str(), &helper_arguments)?;
    read_parent_result(
        &descriptor.result_file,
        &operation_id,
        &descriptor.target_version,
        exit_code,
    )
}

fn read_parent_result(
    result_file: &Path,
    expected_operation_id: &str,
    expected_version: &str,
    exit_code: u32,
) -> Result<AddonTransactionReport, String> {
    let bytes = fs::read(result_file).map_err(|error| match exit_code {
        10 => format!(
            "Elevated helper rejected its arguments or request before returning a result: {error}"
        ),
        40 => format!("Elevated helper could not write its result file: {error}"),
        _ => format!(
            "Elevated helper exited with code {exit_code} without a valid result file: {error}"
        ),
    })?;
    let payload: HelperResult = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Elevated helper result is malformed: {error}"))?;
    if payload.schema_version != RESULT_SCHEMA || payload.operation_id != expected_operation_id {
        return Err("Elevated helper returned a stale or mismatched operation result.".to_owned());
    }
    if exit_code != 0 {
        return Err(helper_failure_message(&payload, exit_code));
    }
    if !payload.ok {
        return Err(helper_failure_message(&payload, exit_code));
    }
    let version = payload
        .version
        .ok_or_else(|| "Elevated helper success result has no installed version.".to_owned())?;
    if version != expected_version {
        return Err(format!(
            "Elevated helper installed version {version}, expected {expected_version}."
        ));
    }
    Ok(AddonTransactionReport { version })
}

fn helper_failure_message(payload: &HelperResult, exit_code: u32) -> String {
    let stage = payload.stage.as_deref().unwrap_or("unknown stage");
    match payload.message.as_deref() {
        Some(message) if exit_code == 0 => {
            format!("Elevated helper reported failure at {stage}: {message}")
        }
        Some(message) => {
            format!("Elevated helper failed at {stage}: {message} (exit code {exit_code}).")
        }
        None => format!("Elevated helper failed at {stage} (exit code {exit_code})."),
    }
}

#[cfg(windows)]
fn runas_and_wait(executable: &std::ffi::OsStr, arguments: &str) -> Result<u32, String> {
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
            format!("Could not launch elevated helper: {error}")
        });
    }
    if request.hProcess.is_null() {
        return Err("Elevated helper started without a process handle.".to_owned());
    }
    unsafe { WaitForSingleObject(request.hProcess, INFINITE) };
    let mut exit_code = 0;
    let got_exit_code = unsafe { GetExitCodeProcess(request.hProcess, &mut exit_code) } != 0;
    unsafe { CloseHandle(request.hProcess) };
    if !got_exit_code {
        return Err(format!(
            "Could not read elevated helper exit code: {}",
            io::Error::last_os_error()
        ));
    }
    Ok(exit_code)
}

#[cfg(not(windows))]
pub fn replace_elevated(_: &Path, _: &StagedRelease) -> Result<AddonTransactionReport, String> {
    Err("Administrator elevation is only available on Windows.".to_owned())
}

/// Quotes one argument using Windows CommandLineToArgvW escaping rules.
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

fn build_helper_arguments(operation_id: &str, request_file: &Path) -> String {
    [
        HELPER_FLAG.to_owned(),
        operation_id.to_owned(),
        request_file.to_string_lossy().into_owned(),
    ]
    .iter()
    .map(|argument| quote_windows_argument(argument))
    .collect::<Vec<_>>()
    .join(" ")
}

/// Parses the subset of Windows command-line quoting used by the helper,
/// following CommandLineToArgvW backslash and quote handling.
#[cfg(test)]
fn parse_windows_arguments(command_line: &str) -> Vec<String> {
    let mut arguments = Vec::new();
    let mut chars = command_line.chars().peekable();
    while chars.peek().is_some() {
        while chars
            .peek()
            .is_some_and(|character| character.is_whitespace())
        {
            chars.next();
        }
        if chars.peek().is_none() {
            break;
        }
        let mut argument = String::new();
        let mut in_quotes = false;
        loop {
            let mut slashes = 0;
            while chars.peek() == Some(&'\\') {
                chars.next();
                slashes += 1;
            }
            if chars.peek() == Some(&'"') {
                argument.push_str(&"\\".repeat(slashes / 2));
                chars.next();
                if slashes % 2 == 1 {
                    argument.push('"');
                } else if in_quotes && chars.peek() == Some(&'"') {
                    argument.push('"');
                    chars.next();
                } else {
                    in_quotes = !in_quotes;
                }
                continue;
            }
            argument.push_str(&"\\".repeat(slashes));
            match chars.peek().copied() {
                Some(character) if !in_quotes && character.is_whitespace() => break,
                Some(character) => {
                    argument.push(character);
                    chars.next();
                }
                None => break,
            }
        }
        arguments.push(argument);
    }
    arguments
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_serializes_and_round_trips() {
        let descriptor = OperationDescriptor {
            schema_version: DESCRIPTOR_SCHEMA,
            operation_id: "test-1".to_owned(),
            installation: PathBuf::from("C:/WoW"),
            addon_destination: PathBuf::from("C:/WoW/Interface/AddOns/RPEngine2"),
            staged_directory: PathBuf::from("C:/cache/rpengine-op-test-1/staged/release"),
            target_version: "2.0.alpha5".to_owned(),
            package_identity: "sha256:abc".to_owned(),
            operation_directory: PathBuf::from("C:/cache/rpengine-op-test-1"),
            request_file: PathBuf::from("C:/cache/rpengine-op-test-1/request.json"),
            result_file: PathBuf::from("C:/cache/rpengine-op-test-1/result.json"),
        };
        let decoded: OperationDescriptor =
            serde_json::from_slice(&serde_json::to_vec(&descriptor).expect("serialize descriptor"))
                .expect("deserialize descriptor");
        assert_eq!(decoded, descriptor);
    }

    #[test]
    fn helper_result_requires_matching_operation_id() {
        let result = HelperResult {
            schema_version: RESULT_SCHEMA,
            operation_id: "one".to_owned(),
            ok: true,
            version: Some("2.0.alpha5".to_owned()),
            stage: None,
            message: None,
        };
        assert_eq!(result.operation_id, "one");
        assert_ne!(result.operation_id, "two");
    }

    fn descriptor_fixture() -> (PathBuf, PathBuf, OperationDescriptor) {
        let root = std::env::temp_dir().join(format!("rpengine-helper-{}", operation_id()));
        let manager_root = root.join("cache/release-staging");
        let operation_id = "test-1";
        let operation_directory = manager_root.join(format!("rpengine-op-{operation_id}"));
        let installation = root.join("wow");
        let staged_directory = operation_directory.join("staged/release");
        fs::create_dir_all(staged_directory.join("RPEngine2")).expect("create staged addon");
        fs::create_dir_all(&installation).expect("create installation");
        fs::write(installation.join("Wow.exe"), b"fixture").expect("create WoW executable");
        fs::write(
            staged_directory.join("RPEngine2/RPEngine2.toc"),
            "## Version: 2.0.alpha5\n",
        )
        .expect("write staged toc");
        fs::write(
            staged_directory.join("RPEngine2-2.0.alpha5.zip"),
            b"package",
        )
        .expect("write staged package");
        fs::create_dir_all(&operation_directory).expect("create operation directory");
        fs::write(operation_directory.join("request.json"), b"{}").expect("create request");
        let installation = installation.canonicalize().expect("canonical installation");
        let operation_directory = operation_directory
            .canonicalize()
            .expect("canonical operation directory");
        let staged_directory = staged_directory.canonicalize().expect("canonical stage");
        let manager_root = manager_root.canonicalize().expect("canonical manager root");
        let descriptor = OperationDescriptor {
            schema_version: DESCRIPTOR_SCHEMA,
            operation_id: operation_id.to_owned(),
            installation: installation.clone(),
            addon_destination: installation.join("Interface/AddOns/RPEngine2"),
            staged_directory,
            target_version: "2.0.alpha5".to_owned(),
            package_identity: "sha256:fixture".to_owned(),
            operation_directory: operation_directory.clone(),
            request_file: operation_directory.join("request.json"),
            result_file: operation_directory.join("result.json"),
        };
        (root, manager_root, descriptor)
    }

    #[test]
    fn descriptor_validation_rejects_wrong_schema_id_destination_stage_and_version() {
        let (root, manager_root, mut descriptor) = descriptor_fixture();
        let request_file = descriptor.request_file.clone();
        let operation_id = descriptor.operation_id.clone();
        let validate = |descriptor: &OperationDescriptor| {
            validate_operation_descriptor(descriptor, &operation_id, &request_file, &manager_root)
        };
        validate(&descriptor).expect("valid descriptor");

        descriptor.schema_version += 1;
        assert!(validate(&descriptor)
            .unwrap_err()
            .message
            .contains("schema"));
        descriptor.schema_version = DESCRIPTOR_SCHEMA;
        assert!(validate_operation_descriptor(
            &descriptor,
            "different-id",
            &request_file,
            &manager_root
        )
        .unwrap_err()
        .message
        .contains("Operation ID"));
        descriptor.addon_destination = root.join("outside/RPEngine2");
        assert!(validate(&descriptor)
            .unwrap_err()
            .message
            .contains("outside the selected installation"));
        descriptor.addon_destination = descriptor.installation.join("Interface/AddOns/RPEngine2");
        descriptor.staged_directory = root.join("outside-stage");
        fs::create_dir_all(&descriptor.staged_directory).expect("create outside stage");
        assert!(validate(&descriptor)
            .unwrap_err()
            .message
            .contains("outside this Manager operation"));
        descriptor.staged_directory = descriptor.operation_directory.join("staged/release");
        descriptor.target_version = "not-a-release".to_owned();
        assert!(validate(&descriptor)
            .unwrap_err()
            .message
            .contains("version is invalid"));
        descriptor.target_version = "2.0.alpha4".to_owned();
        fs::write(
            &descriptor.request_file,
            serde_json::to_vec(&descriptor).expect("serialize mismatched request"),
        )
        .expect("write mismatched request");
        assert!(
            read_and_validate_operation(&operation_id, &request_file, &manager_root)
                .err()
                .expect("mismatched target version is rejected")
                .message
                .contains("does not match release")
        );
        fs::remove_dir_all(root).expect("remove descriptor fixture");
    }

    fn write_test_result(path: &Path, operation_id: &str, ok: bool) {
        let result = HelperResult {
            schema_version: RESULT_SCHEMA,
            operation_id: operation_id.to_owned(),
            ok,
            version: ok.then(|| "2.0.alpha5".to_owned()),
            stage: (!ok).then(|| "transaction.activation".to_owned()),
            message: (!ok).then(|| "Could not activate staged addon".to_owned()),
        };
        fs::write(path, serde_json::to_vec(&result).expect("serialize result"))
            .expect("write result fixture");
    }

    #[test]
    fn parent_requires_a_success_result_for_the_requested_operation() {
        let root = std::env::temp_dir().join(format!("rpengine-result-{}", operation_id()));
        fs::create_dir_all(&root).expect("create result fixture");
        let result_file = root.join("result.json");
        write_test_result(&result_file, "op-one", true);
        assert_eq!(
            read_parent_result(&result_file, "op-one", "2.0.alpha5", 0)
                .expect("valid result")
                .version,
            "2.0.alpha5"
        );
        assert!(read_parent_result(&result_file, "op-two", "2.0.alpha5", 0)
            .unwrap_err()
            .contains("mismatched"));
        assert!(read_parent_result(&result_file, "op-one", "2.0.alpha5", 20).is_err());
        fs::remove_dir_all(root).expect("remove result fixture");
    }

    #[test]
    fn parent_propagates_failure_and_rejects_missing_or_malformed_results() {
        let root = std::env::temp_dir().join(format!("rpengine-result-{}", operation_id()));
        fs::create_dir_all(&root).expect("create result fixture");
        let result_file = root.join("result.json");
        write_test_result(&result_file, "op-one", false);
        assert_eq!(
            read_parent_result(&result_file, "op-one", "2.0.alpha5", 20).unwrap_err(),
            "Elevated helper failed at transaction.activation: Could not activate staged addon (exit code 20)."
        );
        fs::write(&result_file, b"not json").expect("write malformed result");
        assert!(read_parent_result(&result_file, "op-one", "2.0.alpha5", 20)
            .unwrap_err()
            .contains("malformed"));
        fs::remove_file(&result_file).expect("remove malformed result");
        assert!(read_parent_result(&result_file, "op-one", "2.0.alpha5", 40)
            .unwrap_err()
            .contains("could not write"));
        fs::remove_dir_all(root).expect("remove result fixture");
    }

    #[test]
    fn helper_returns_failure_when_mandatory_result_write_fails() {
        let root = std::env::temp_dir().join(format!("rpengine-helper-write-{}", operation_id()));
        let operation_id = "test-write";
        let operation = root.join(format!("rpengine-op-{operation_id}"));
        fs::create_dir_all(&operation).expect("create helper fixture");
        fs::write(operation.join("request.json"), b"malformed").expect("write request");
        fs::create_dir(operation.join("result.json")).expect("block result file creation");
        assert_eq!(
            run_helper_request_under_root(operation_id, &operation.join("request.json"), &root),
            40
        );
        fs::remove_dir_all(root).expect("remove helper fixture");
    }

    #[test]
    fn helper_argument_paths_round_trip_through_windows_quoting() {
        let cases = [
            "C:\\Program Files (x86)\\World of Warcraft\\",
            "\\\\?\\C:\\RPEngine\\",
            "C:\\Users\\Élodie\\缓存\\",
            "D:\\staged\\rpengine-op-x\\request.json",
        ];
        for path in cases {
            let command_line = build_helper_arguments("op-42", Path::new(path));
            let arguments = parse_windows_arguments(&command_line);
            assert_eq!(arguments[0], HELPER_FLAG);
            assert_eq!(arguments[1], "op-42");
            assert_eq!(arguments[2], path);
        }
        let quoted = quote_windows_argument("a\"b");
        assert_eq!(parse_windows_arguments(&quoted), vec!["a\"b"]);
    }
}
