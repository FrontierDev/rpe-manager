//! Atomic whole-directory replacement of the RPEngine addon.

use std::{
    fmt, fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Serialize;

use crate::{
    discovery::rpengine::{discover_rpengine, RPEngineInstallationStatus},
    processes::wow::{WowModificationSafetyError, WowModificationSafetyState},
    release::{validate_staged_release, ReleaseError, StagedRelease},
};

const ADDON_PARENT: &str = "Interface/AddOns";
const ADDON_NAME: &str = "RPEngine2";

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddonTransactionReport {
    pub version: String,
}

#[derive(Debug)]
pub enum AddonTransactionError {
    GameRunning(WowModificationSafetyError),
    StagedRelease(ReleaseError),
    InvalidInstallation(PathBuf),
    TargetNotWritable {
        path: PathBuf,
        source: io::Error,
    },
    CopyStaged {
        path: PathBuf,
        source: io::Error,
    },
    DisplaceExisting {
        path: PathBuf,
        source: io::Error,
    },
    ActivateNew {
        path: PathBuf,
        source: io::Error,
    },
    BackupCleanup {
        path: PathBuf,
        source: io::Error,
    },
    Verification(String),
    Rollback {
        primary: Box<AddonTransactionError>,
        rollback: String,
    },
}

impl fmt::Display for AddonTransactionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GameRunning(error) => error.fmt(f),
            Self::StagedRelease(error) => error.fmt(f),
            Self::InvalidInstallation(path) => write!(
                f,
                "Selected WoW installation {} is not a directory.",
                path.display()
            ),
            Self::TargetNotWritable { path, source } => write!(
                f,
                "Addon parent {} is not writable: {source}",
                path.display()
            ),
            Self::CopyStaged { path, source } => write!(
                f,
                "Could not prepare staged addon at {}: {source}",
                path.display()
            ),
            Self::DisplaceExisting { path, source } => write!(
                f,
                "Could not move existing addon {} aside: {source}",
                path.display()
            ),
            Self::ActivateNew { path, source } => write!(
                f,
                "Could not activate new addon at {}: {source}",
                path.display()
            ),
            Self::BackupCleanup { path, source } => write!(
                f,
                "Installed addon verified, but transaction backup {} could not be removed: {source}",
                path.display()
            ),
            Self::Verification(message) => {
                write!(f, "Installed addon verification failed: {message}")
            }
            Self::Rollback { primary, rollback } => {
                write!(f, "{primary}; rollback also failed: {rollback}")
            }
        }
    }
}
impl std::error::Error for AddonTransactionError {}

pub fn replace_staged_addon(
    installation: &Path,
    staged: &StagedRelease,
    safety: WowModificationSafetyState,
) -> Result<AddonTransactionReport, AddonTransactionError> {
    replace_staged_addon_with(installation, staged, safety, verify_installation)
}

/// Shared elevated entry point for the helper and an already-elevated Manager.
/// The helper validates its operation descriptor before calling this path.
pub fn replace_staged_addon_privileged(
    installation: &Path,
    staged: &StagedRelease,
) -> Result<AddonTransactionReport, AddonTransactionError> {
    if crate::discovery::wow::validate_installation_path(installation).is_err() {
        return Err(AddonTransactionError::InvalidInstallation(
            installation.to_path_buf(),
        ));
    }
    replace_staged_addon(
        installation,
        staged,
        crate::processes::wow::inspect_wow_processes(),
    )
}

fn replace_staged_addon_with<F>(
    installation: &Path,
    staged: &StagedRelease,
    safety: WowModificationSafetyState,
    verify: F,
) -> Result<AddonTransactionReport, AddonTransactionError>
where
    F: Fn(&Path, &str) -> Result<(), AddonTransactionError>,
{
    replace_staged_addon_with_copy(installation, staged, safety, copy_directory, verify)
}

fn replace_staged_addon_with_copy<C, F>(
    installation: &Path,
    staged: &StagedRelease,
    safety: WowModificationSafetyState,
    copy: C,
    verify: F,
) -> Result<AddonTransactionReport, AddonTransactionError>
where
    C: Fn(&Path, &Path) -> io::Result<()>,
    F: Fn(&Path, &str) -> Result<(), AddonTransactionError>,
{
    safety
        .require_modification_allowed()
        .map_err(AddonTransactionError::GameRunning)?;
    validate_staged_release(staged).map_err(AddonTransactionError::StagedRelease)?;
    let parent = prepare_addon_parent(installation)?;
    let destination = parent.join(ADDON_NAME);
    let nonce = transaction_nonce();
    let backup = parent.join(format!(".{ADDON_NAME}.backup-{nonce}"));

    let had_existing =
        destination
            .try_exists()
            .map_err(|source| AddonTransactionError::TargetNotWritable {
                path: destination.clone(),
                source,
            })?;
    if had_existing {
        fs::rename(&destination, &backup).map_err(|source| {
            AddonTransactionError::DisplaceExisting {
                path: destination.clone(),
                source,
            }
        })?;
    }
    // The validated staging tree is Manager-owned and deliberately outside
    // the game installation. Copy only after the old directory has moved;
    // this avoids persistent .incoming directories under Interface/AddOns.
    if let Err(primary) = copy(&staged.directory.join(ADDON_NAME), &destination)
        .map_err(|source| AddonTransactionError::ActivateNew {
            path: destination.clone(),
            source,
        })
        .and_then(|_| verify(installation, &staged.version))
    {
        return rollback(&destination, &backup, had_existing, primary);
    }
    if had_existing {
        fs::remove_dir_all(&backup).map_err(|source| AddonTransactionError::BackupCleanup {
            path: backup,
            source,
        })?;
    }
    Ok(AddonTransactionReport {
        version: staged.version.clone(),
    })
}

fn verify_installation(installation: &Path, expected: &str) -> Result<(), AddonTransactionError> {
    let found = discover_rpengine(installation)
        .map_err(|error| AddonTransactionError::Verification(error.to_string()))?;
    if found.status != RPEngineInstallationStatus::Installed
        || found.version.as_deref() != Some(expected)
    {
        return Err(AddonTransactionError::Verification(format!(
            "expected healthy version {expected}"
        )));
    }
    Ok(())
}

fn rollback<T>(
    destination: &Path,
    backup: &Path,
    had_existing: bool,
    primary: AddonTransactionError,
) -> Result<T, AddonTransactionError> {
    let cleanup = || -> Result<(), io::Error> {
        if destination.try_exists()? {
            fs::remove_dir_all(destination)?;
        }
        if had_existing {
            fs::rename(backup, destination)?;
            let metadata = fs::symlink_metadata(destination)?;
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                return Err(io::Error::other("restored addon is not a directory"));
            }
        }
        Ok(())
    };
    match cleanup() {
        Ok(()) => Err(primary),
        Err(error) => Err(AddonTransactionError::Rollback {
            primary: Box::new(primary),
            rollback: format!(
                "could not clean partial destination {} and restore backup {}: {error}",
                destination.display(),
                backup.display()
            ),
        }),
    }
}

fn prepare_addon_parent(installation: &Path) -> Result<PathBuf, AddonTransactionError> {
    if !fs::metadata(installation)
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false)
    {
        return Err(AddonTransactionError::InvalidInstallation(
            installation.to_path_buf(),
        ));
    }
    let parent = installation.join(ADDON_PARENT);
    fs::create_dir_all(&parent).map_err(|source| AddonTransactionError::TargetNotWritable {
        path: parent.clone(),
        source,
    })?;
    Ok(parent)
}

fn copy_directory(source: &Path, destination: &Path) -> io::Result<()> {
    fs::create_dir(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            copy_directory(&entry.path(), &target)?;
        } else if metadata.is_file() {
            fs::copy(entry.path(), target)?;
        } else {
            return Err(io::Error::other("staged addon contains a non-file entry"));
        }
    }
    Ok(())
}

fn transaction_nonce() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::processes::wow::safety_state_from_process_names;

    fn fixture_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "rpengine-transaction-{name}-{}",
            transaction_nonce()
        ));
        fs::create_dir_all(&root).expect("create fixture root");
        root
    }

    fn staged(root: &Path, version: &str) -> StagedRelease {
        let directory = root.join(format!("stage-{version}"));
        let addon = directory.join(ADDON_NAME);
        fs::create_dir_all(&addon).expect("create staged addon");
        fs::write(
            addon.join("RPEngine2.toc"),
            format!("## Version: {version}\n"),
        )
        .expect("write staged toc");
        fs::write(addon.join("new.lua"), "new").expect("write staged addon file");
        StagedRelease {
            version: version.to_owned(),
            directory,
        }
    }

    fn installation(root: &Path) -> PathBuf {
        let installation = root.join("wow");
        fs::create_dir_all(&installation).expect("create installation");
        installation
    }

    fn safe() -> WowModificationSafetyState {
        safety_state_from_process_names(Vec::<String>::new())
    }

    fn addon(installation: &Path) -> PathBuf {
        installation.join(ADDON_PARENT).join(ADDON_NAME)
    }

    fn old_addon(installation: &Path, version: &str) {
        let addon = addon(installation);
        fs::create_dir_all(&addon).expect("create old addon");
        fs::write(
            addon.join("RPEngine2.toc"),
            format!("## Version: {version}\n"),
        )
        .expect("write old toc");
        fs::write(addon.join("stale.lua"), "old").expect("write stale file");
        fs::create_dir_all(addon.join("nested")).expect("create nested old files");
        fs::write(addon.join("nested/old.dat"), b"exact old bytes").expect("write nested old file");
    }

    fn tree_contents(root: &Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
        fn collect(
            root: &Path,
            directory: &Path,
            files: &mut std::collections::BTreeMap<PathBuf, Vec<u8>>,
        ) {
            for entry in fs::read_dir(directory).expect("read tree") {
                let path = entry.expect("read entry").path();
                if path.is_dir() {
                    collect(root, &path, files);
                } else {
                    files.insert(
                        path.strip_prefix(root)
                            .expect("relative path")
                            .to_path_buf(),
                        fs::read(path).expect("read file"),
                    );
                }
            }
        }

        let mut files = std::collections::BTreeMap::new();
        collect(root, root, &mut files);
        files
    }

    #[test]
    fn fresh_install_places_the_staged_addon() {
        let root = fixture_root("fresh");
        let installation = installation(&root);
        let staged = staged(&root, "2.0.alpha5");
        let report = replace_staged_addon(&installation, &staged, safe()).expect("fresh install");
        assert_eq!(report.version, "2.0.alpha5");
        assert!(addon(&installation).join("new.lua").is_file());
        fs::remove_dir_all(root).expect("remove fixture");
    }

    #[test]
    fn update_and_repair_use_whole_directory_replacement() {
        let root = fixture_root("replace");
        let installation = installation(&root);
        let staged = staged(&root, "2.0.alpha5");
        old_addon(&installation, "2.0.alpha4");
        replace_staged_addon(&installation, &staged, safe()).expect("update");
        assert!(!addon(&installation).join("stale.lua").exists());
        assert_eq!(
            discover_rpengine(&installation)
                .expect("discover update")
                .version
                .as_deref(),
            Some("2.0.alpha5")
        );
        fs::remove_file(addon(&installation).join("RPEngine2.toc")).expect("damage addon");
        replace_staged_addon(&installation, &staged, safe()).expect("repair");
        assert_eq!(
            discover_rpengine(&installation)
                .expect("discover repair")
                .status,
            RPEngineInstallationStatus::Installed
        );
        fs::remove_dir_all(root).expect("remove fixture");
    }

    #[test]
    fn running_wow_or_invalid_staging_cannot_modify_the_live_addon() {
        let root = fixture_root("refusal");
        let installation = installation(&root);
        old_addon(&installation, "2.0.alpha4");
        let original = tree_contents(&addon(&installation));
        let staged = staged(&root, "2.0.alpha5");
        let running = safety_state_from_process_names(["Wow.exe"]);
        assert!(matches!(
            replace_staged_addon(&installation, &staged, running),
            Err(AddonTransactionError::GameRunning(_))
        ));
        assert!(addon(&installation).join("stale.lua").is_file());
        fs::write(
            staged.directory.join(ADDON_NAME).join("RPEngine2.toc"),
            "## Version: 2.0.alpha4\n",
        )
        .expect("corrupt staging");
        assert!(matches!(
            replace_staged_addon(&installation, &staged, safe()),
            Err(AddonTransactionError::StagedRelease(_))
        ));
        assert_eq!(
            discover_rpengine(&installation)
                .expect("discover original")
                .version
                .as_deref(),
            Some("2.0.alpha4")
        );
        assert_eq!(tree_contents(&addon(&installation)), original);
        fs::remove_dir_all(root).expect("remove fixture");
    }

    #[test]
    fn invalid_target_is_refused_before_staged_content_is_copied() {
        let root = fixture_root("invalid-target");
        let staged = staged(&root, "2.0.alpha5");
        let missing_installation = root.join("does-not-exist");
        assert!(matches!(
            replace_staged_addon(&missing_installation, &staged, safe()),
            Err(AddonTransactionError::InvalidInstallation(_))
        ));
        assert!(!missing_installation.exists());
        fs::remove_dir_all(root).expect("remove fixture");
    }

    #[test]
    fn post_replacement_failure_rolls_back_old_content_and_leaves_wtf_unchanged() {
        let root = fixture_root("rollback");
        let installation = installation(&root);
        old_addon(&installation, "2.0.alpha4");
        let original = tree_contents(&addon(&installation));
        let wtf = installation.join("WTF/Account/TEST/SavedVariables/RPEngine2.lua");
        fs::create_dir_all(wtf.parent().expect("wtf parent")).expect("create wtf");
        fs::write(&wtf, "unchanged bytes").expect("write wtf");
        let staged = staged(&root, "2.0.alpha5");
        let result = replace_staged_addon_with(&installation, &staged, safe(), |path, _| {
            verify_installation(path, "2.0.alpha4")
        });
        assert!(matches!(
            result,
            Err(AddonTransactionError::Verification(_))
        ));
        assert_eq!(
            discover_rpengine(&installation)
                .expect("discover restored")
                .version
                .as_deref(),
            Some("2.0.alpha4")
        );
        assert!(addon(&installation).join("stale.lua").is_file());
        assert_eq!(tree_contents(&addon(&installation)), original);
        assert_eq!(
            fs::read_to_string(&wtf).expect("read wtf"),
            "unchanged bytes"
        );
        fs::remove_dir_all(root).expect("remove fixture");
    }

    #[test]
    fn activation_failure_restores_the_exact_old_addon() {
        let root = fixture_root("activation-rollback");
        let installation = installation(&root);
        old_addon(&installation, "2.0.alpha4");
        let original = tree_contents(&addon(&installation));
        let staged = staged(&root, "2.0.alpha5");
        let result = replace_staged_addon_with_copy(
            &installation,
            &staged,
            safe(),
            |_, _| Err(io::Error::other("forced activation failure")),
            verify_installation,
        );
        assert!(matches!(
            result,
            Err(AddonTransactionError::ActivateNew { .. })
        ));
        assert_eq!(tree_contents(&addon(&installation)), original);
        assert!(fs::read_dir(installation.join(ADDON_PARENT))
            .expect("read addon parent")
            .all(|entry| !entry
                .expect("read addon entry")
                .file_name()
                .to_string_lossy()
                .starts_with(".RPEngine2.backup-")));
        fs::remove_dir_all(root).expect("remove fixture");
    }

    #[test]
    fn failed_fresh_install_cleans_the_destination() {
        let root = fixture_root("fresh-failure");
        let installation = installation(&root);
        let staged = staged(&root, "2.0.alpha5");
        let result = replace_staged_addon_with(&installation, &staged, safe(), |_, _| {
            Err(AddonTransactionError::Verification(
                "forced failure".to_owned(),
            ))
        });
        assert!(matches!(
            result,
            Err(AddonTransactionError::Verification(_))
        ));
        assert!(!addon(&installation).exists());
        fs::remove_dir_all(root).expect("remove fixture");
    }

    #[test]
    fn rollback_failure_is_terminal_and_reported() {
        let root = fixture_root("rollback-failure");
        let destination = root.join("RPEngine2");
        fs::create_dir_all(&destination).expect("create partial destination");
        fs::write(destination.join("partial.lua"), "partial").expect("write partial addon");
        let missing_backup = root.join("missing-backup");
        let result = rollback::<()>(
            &destination,
            &missing_backup,
            true,
            AddonTransactionError::Verification("forced version mismatch".to_owned()),
        );
        assert!(matches!(
            result,
            Err(AddonTransactionError::Rollback { .. })
        ));
        assert!(!destination.exists());
        fs::remove_dir_all(root).expect("remove fixture");
    }
}
