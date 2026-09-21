#![cfg(windows)]

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use rpengine_manager_lib::elevation::{staged_package_identity, OperationDescriptor};
use rpengine_manager_lib::processes::wow::inspect_wow_processes;

const MANAGER_IDENTIFIER: &str = "net.esarus.rpengine-manager";

struct Cleanup(Vec<PathBuf>);

impl Drop for Cleanup {
    fn drop(&mut self) {
        for path in self.0.iter().rev() {
            let _ = fs::remove_dir_all(path);
        }
    }
}

fn operation_id() -> String {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    format!("integration-{}-{nonce}", std::process::id())
}

#[test]
fn helper_subprocess_returns_operation_result_and_obeys_modification_safety() {
    let id = operation_id();
    let local_app_data = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!("helper-localappdata-{id}"));
    let manager_root = local_app_data
        .join(MANAGER_IDENTIFIER)
        .join("release-staging");
    fs::create_dir_all(&manager_root).expect("create Manager staging root");
    let operation_directory = manager_root.join(format!("rpengine-op-{id}"));
    let temporary_installation = env::temp_dir().join(format!("World of Warcraft 测试-{id}"));
    let cleanup = Cleanup(vec![local_app_data.clone(), temporary_installation.clone()]);

    let staged_directory = operation_directory.join("staged/release");
    fs::create_dir_all(staged_directory.join("RPEngine2")).expect("create stage");
    fs::write(
        staged_directory.join("RPEngine2/RPEngine2.toc"),
        "## Version: 2.0.alpha5\n",
    )
    .expect("write staged toc");
    fs::write(staged_directory.join("RPEngine2/new.lua"), "new addon").expect("write new addon");
    fs::write(
        staged_directory.join("RPEngine2-2.0.alpha5.zip"),
        b"fixture package",
    )
    .expect("write package identity source");

    fs::create_dir_all(&temporary_installation).expect("create WoW fixture");
    fs::write(
        temporary_installation.join("Wow.exe"),
        b"fixture executable",
    )
    .expect("create WoW executable marker");
    let live_addon = temporary_installation.join("Interface/AddOns/RPEngine2");
    fs::create_dir_all(&live_addon).expect("create existing addon");
    fs::write(live_addon.join("RPEngine2.toc"), "## Version: 2.0.alpha4\n").expect("write old toc");
    fs::write(live_addon.join("old-only.lua"), "stale addon file").expect("write old-only file");

    let operation_directory = operation_directory
        .canonicalize()
        .expect("canonical operation");
    let installation = temporary_installation
        .canonicalize()
        .expect("canonical installation");
    let staged_directory = staged_directory.canonicalize().expect("canonical stage");
    let descriptor = OperationDescriptor {
        schema_version: 1,
        operation_id: id.clone(),
        installation: installation.clone(),
        addon_destination: installation.join("Interface/AddOns/RPEngine2"),
        staged_directory: staged_directory.clone(),
        target_version: "2.0.alpha5".to_owned(),
        package_identity: staged_package_identity(&staged_directory).expect("stage identity"),
        operation_directory: operation_directory.clone(),
        request_file: operation_directory.join("request.json"),
        result_file: operation_directory.join("result.json"),
    };
    fs::write(
        &descriptor.request_file,
        serde_json::to_vec_pretty(&descriptor).expect("serialize operation request"),
    )
    .expect("write operation request");

    let wow_was_running = inspect_wow_processes().is_wow_running;
    let executable = env!("CARGO_BIN_EXE_rpengine-manager");
    let output = Command::new(executable)
        .env("LOCALAPPDATA", &local_app_data)
        .args([
            "--rpengine-addon-helper",
            &id,
            descriptor
                .request_file
                .to_str()
                .expect("request path is Unicode"),
        ])
        .output()
        .expect("launch helper without elevation");
    let helper_result = fs::read_to_string(&descriptor.result_file)
        .unwrap_or_else(|error| format!("result file unavailable: {error}"));
    let result: serde_json::Value =
        serde_json::from_str(&helper_result).expect("parse helper result");
    assert_eq!(result["operationId"], id);
    if wow_was_running {
        assert_eq!(output.status.code(), Some(20));
        assert_eq!(result["ok"], false);
        assert_eq!(result["stage"], "transaction.game_running");
        assert!(live_addon.join("old-only.lua").is_file());
        assert_eq!(
            fs::read_to_string(live_addon.join("RPEngine2.toc")).expect("read old toc"),
            "## Version: 2.0.alpha4\n"
        );
    } else {
        assert!(
            output.status.success(),
            "helper failed with {:?}: result={}, stdout={}, stderr={}",
            output.status.code(),
            helper_result,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(result["ok"], true);
        assert_eq!(result["version"], "2.0.alpha5");
        assert!(live_addon.join("new.lua").is_file());
        assert!(!live_addon.join("old-only.lua").exists());
        assert!(
            fs::read_dir(temporary_installation.join("Interface/AddOns"))
                .expect("read addon directory")
                .all(|entry| !entry
                    .expect("read addon entry")
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".RPEngine2.backup-"))
        );
    }

    drop(cleanup);
    let _ = remove_empty_directory(&manager_root);
}

fn remove_empty_directory(path: &Path) -> std::io::Result<()> {
    if fs::read_dir(path)?.next().is_none() {
        fs::remove_dir(path)?;
    }
    Ok(())
}
