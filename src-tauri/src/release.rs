//! Trusted acquisition and staging of official RPEngine release packages.

use std::{
    cmp::Ordering,
    collections::HashSet,
    fmt,
    fs::{self, File},
    io::{self, Cursor},
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Deserialize;
use sha2::{Digest, Sha256};
use zip::ZipArchive;

use crate::discovery::rpengine::parse_version as parse_toc_version;

pub const LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/FrontierDev/rpe2/releases/latest";
const PACKAGE_DIRECTORY: &str = "RPEngine2";
const TOC_NAME: &str = "RPEngine2.toc";

#[derive(Clone, Debug, Deserialize)]
pub struct GitHubRelease {
    pub tag_name: String,
    pub name: Option<String>,
    pub published_at: Option<String>,
    pub assets: Vec<GitHubAsset>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct GitHubAsset {
    pub name: String,
    pub browser_download_url: String,
    pub digest: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RpeVersion {
    major: u32,
    minor: u32,
    channel: VersionChannel,
    channel_number: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum VersionChannel {
    Alpha,
    Final,
}

impl RpeVersion {
    pub fn parse(value: &str) -> Result<Self, VersionError> {
        let parts: Vec<_> = value.split('.').collect();
        let [major, minor, channel] = parts.as_slice() else {
            return Err(VersionError(value.to_owned()));
        };
        let (kind, number) = if let Some(number) = channel.strip_prefix("alpha") {
            (VersionChannel::Alpha, number.parse().ok())
        } else if channel.is_empty() {
            (VersionChannel::Final, Some(0))
        } else {
            (VersionChannel::Final, None)
        };
        Ok(Self {
            major: major.parse().map_err(|_| VersionError(value.to_owned()))?,
            minor: minor.parse().map_err(|_| VersionError(value.to_owned()))?,
            channel: kind,
            channel_number: number.ok_or_else(|| VersionError(value.to_owned()))?,
        })
    }
}

impl Ord for RpeVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.channel, self.channel_number).cmp(&(
            other.major,
            other.minor,
            other.channel,
            other.channel_number,
        ))
    }
}
impl PartialOrd for RpeVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VersionError(String);
impl fmt::Display for VersionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unsupported RPEngine version {:?}", self.0)
    }
}
impl std::error::Error for VersionError {}

#[derive(Clone, Debug)]
pub struct ReleaseAssets<'a> {
    pub zip: &'a GitHubAsset,
    pub checksum: &'a GitHubAsset,
}

#[derive(Debug)]
pub enum ReleaseError {
    InvalidTag(VersionError),
    MissingAsset(&'static str),
    DuplicateAsset(String),
    MalformedAsset(String),
    MalformedChecksum,
    ChecksumMismatch,
    GitHubDigestMalformed,
    GitHubDigestMismatch,
    Download {
        asset: String,
        source: reqwest::Error,
    },
    CreateStorage {
        path: PathBuf,
        source: io::Error,
    },
    WriteStorage {
        path: PathBuf,
        source: io::Error,
    },
    OpenArchive(zip::result::ZipError),
    UnsafeArchivePath(String),
    DuplicateArchivePath(String),
    UnsupportedArchiveEntry(String),
    MissingToc,
    ReadToc {
        path: PathBuf,
        source: io::Error,
    },
    InvalidTocVersion,
    TocVersionMismatch {
        expected: String,
        found: String,
    },
    Extract {
        path: PathBuf,
        source: io::Error,
    },
}

impl fmt::Display for ReleaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTag(e) => e.fmt(f),
            Self::MissingAsset(kind) => write!(f, "release is missing its {kind} asset"),
            Self::DuplicateAsset(name) => write!(f, "release has duplicate asset {name}"),
            Self::MalformedAsset(name) => {
                write!(f, "release asset {name} has no valid HTTPS download URL")
            }
            Self::MalformedChecksum => write!(f, "release checksum is malformed"),
            Self::ChecksumMismatch => write!(f, "downloaded package checksum does not match"),
            Self::GitHubDigestMalformed => write!(f, "GitHub asset digest is malformed"),
            Self::GitHubDigestMismatch => {
                write!(f, "GitHub asset digest disagrees with checksum asset")
            }
            Self::Download { asset, source } => write!(f, "could not download {asset}: {source}"),
            Self::CreateStorage { path, source } => {
                write!(f, "could not create {}: {source}", path.display())
            }
            Self::WriteStorage { path, source } => {
                write!(f, "could not write {}: {source}", path.display())
            }
            Self::OpenArchive(e) => write!(f, "could not open release archive: {e}"),
            Self::UnsafeArchivePath(path) => write!(f, "unsafe archive path {path}"),
            Self::DuplicateArchivePath(path) => write!(f, "duplicate archive path {path}"),
            Self::UnsupportedArchiveEntry(path) => write!(f, "unsupported archive entry {path}"),
            Self::MissingToc => write!(f, "archive does not contain RPEngine2/RPEngine2.toc"),
            Self::ReadToc { path, source } => {
                write!(f, "could not read {}: {source}", path.display())
            }
            Self::InvalidTocVersion => write!(f, "staged TOC has no usable Version field"),
            Self::TocVersionMismatch { expected, found } => write!(
                f,
                "staged TOC version {found} does not match release {expected}"
            ),
            Self::Extract { path, source } => {
                write!(f, "could not extract {}: {source}", path.display())
            }
        }
    }
}
impl std::error::Error for ReleaseError {}

pub fn fetch_latest_release() -> Result<GitHubRelease, String> {
    reqwest::blocking::Client::builder()
        .user_agent("RPEngine-Manager")
        .build()
        .map_err(|e| e.to_string())?
        .get(LATEST_RELEASE_URL)
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .map_err(|e| e.to_string())
}

pub fn select_release_assets(release: &GitHubRelease) -> Result<ReleaseAssets<'_>, ReleaseError> {
    RpeVersion::parse(&release.tag_name).map_err(ReleaseError::InvalidTag)?;
    let zip_name = format!("RPEngine2-{}.zip", release.tag_name);
    let checksum_name = format!("{zip_name}.sha256");
    let mut zip = None;
    let mut checksum = None;
    for asset in &release.assets {
        if asset.name == zip_name && zip.replace(asset).is_some() {
            return Err(ReleaseError::DuplicateAsset(zip_name));
        }
        if asset.name == checksum_name && checksum.replace(asset).is_some() {
            return Err(ReleaseError::DuplicateAsset(checksum_name));
        }
    }
    let zip = zip.ok_or(ReleaseError::MissingAsset("ZIP"))?;
    let checksum = checksum.ok_or(ReleaseError::MissingAsset("checksum"))?;
    for asset in [zip, checksum] {
        let valid_url = reqwest::Url::parse(&asset.browser_download_url)
            .map(|url| url.scheme() == "https")
            .unwrap_or(false);
        if !valid_url {
            return Err(ReleaseError::MalformedAsset(asset.name.clone()));
        }
    }
    Ok(ReleaseAssets { zip, checksum })
}

pub fn parse_checksum(contents: &str, version: &str) -> Result<String, ReleaseError> {
    let filename = format!("RPEngine2-{version}.zip");
    let expected = format!("  {filename}");
    let line = contents.strip_suffix('\n').unwrap_or(contents);
    let line = line.strip_suffix('\r').unwrap_or(line);
    let Some((digest, name)) = line.split_once(&expected) else {
        return Err(ReleaseError::MalformedChecksum);
    };
    if !name.is_empty()
        || digest.len() != 64
        || !digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(ReleaseError::MalformedChecksum);
    }
    Ok(digest.to_owned())
}

pub fn verify_package(
    zip: &[u8],
    checksum: &str,
    github_digest: Option<&str>,
) -> Result<(), ReleaseError> {
    let actual = hex_digest(zip);
    if actual != checksum {
        return Err(ReleaseError::ChecksumMismatch);
    }
    if let Some(digest) = github_digest {
        let Some(github) = digest.strip_prefix("sha256:") else {
            return Err(ReleaseError::GitHubDigestMalformed);
        };
        if github.len() != 64
            || !github
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(ReleaseError::GitHubDigestMalformed);
        }
        if github != checksum {
            return Err(ReleaseError::GitHubDigestMismatch);
        }
    }
    Ok(())
}

pub fn download_and_stage_release(
    release: &GitHubRelease,
    temporary_root: &Path,
) -> Result<StagedRelease, ReleaseError> {
    let assets = select_release_assets(release)?;
    let client = reqwest::blocking::Client::builder()
        .user_agent("RPEngine-Manager")
        .build()
        .map_err(|e| ReleaseError::Download {
            asset: "client".to_owned(),
            source: e,
        })?;
    let zip = client
        .get(&assets.zip.browser_download_url)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|source| ReleaseError::Download {
            asset: assets.zip.name.clone(),
            source,
        })?
        .bytes()
        .map_err(|source| ReleaseError::Download {
            asset: assets.zip.name.clone(),
            source,
        })?;
    let checksum = client
        .get(&assets.checksum.browser_download_url)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|source| ReleaseError::Download {
            asset: assets.checksum.name.clone(),
            source,
        })?
        .bytes()
        .map_err(|source| ReleaseError::Download {
            asset: assets.checksum.name.clone(),
            source,
        })?;
    stage_release_bytes(
        release,
        zip.as_ref(),
        checksum.as_ref(),
        assets.zip.digest.as_deref(),
        temporary_root,
    )
}

pub struct StagedRelease {
    pub version: String,
    pub directory: PathBuf,
}

/// Rechecks the staged tree immediately before it is allowed to replace a live addon.
pub fn validate_staged_release(staged: &StagedRelease) -> Result<(), ReleaseError> {
    RpeVersion::parse(&staged.version).map_err(ReleaseError::InvalidTag)?;
    let toc_path = staged.directory.join(PACKAGE_DIRECTORY).join(TOC_NAME);
    let toc = fs::read_to_string(&toc_path).map_err(|source| ReleaseError::ReadToc {
        path: toc_path,
        source,
    })?;
    let found = parse_toc_version(&toc).ok_or(ReleaseError::InvalidTocVersion)?;
    if found != staged.version {
        return Err(ReleaseError::TocVersionMismatch {
            expected: staged.version.clone(),
            found,
        });
    }
    Ok(())
}

pub fn stage_release_bytes(
    release: &GitHubRelease,
    zip: &[u8],
    checksum: &[u8],
    github_digest: Option<&str>,
    temporary_root: &Path,
) -> Result<StagedRelease, ReleaseError> {
    select_release_assets(release)?;
    let checksum = std::str::from_utf8(checksum).map_err(|_| ReleaseError::MalformedChecksum)?;
    let expected = parse_checksum(checksum, &release.tag_name)?;
    verify_package(zip, &expected, github_digest)?;
    let directory = unique_staging_directory(temporary_root);
    fs::create_dir_all(&directory).map_err(|source| ReleaseError::CreateStorage {
        path: directory.clone(),
        source,
    })?;
    let zip_path = directory.join(format!("RPEngine2-{}.zip", release.tag_name));
    fs::write(&zip_path, zip).map_err(|source| ReleaseError::WriteStorage {
        path: zip_path,
        source,
    })?;
    let checksum_path = directory.join(format!("RPEngine2-{}.zip.sha256", release.tag_name));
    fs::write(&checksum_path, checksum.as_bytes()).map_err(|source| {
        ReleaseError::WriteStorage {
            path: checksum_path,
            source,
        }
    })?;
    extract_canonical_archive(zip, &directory)?;
    let staged = StagedRelease {
        version: release.tag_name.clone(),
        directory,
    };
    validate_staged_release(&staged)?;
    Ok(staged)
}

fn hex_digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn unique_staging_directory(root: &Path) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    root.join(format!("rpengine-stage-{}-{nonce}", std::process::id()))
}

fn extract_canonical_archive(bytes: &[u8], destination: &Path) -> Result<(), ReleaseError> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(ReleaseError::OpenArchive)?;
    let mut paths = HashSet::new();
    let mut toc = false;
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(ReleaseError::OpenArchive)?;
        let path = canonical_archive_path(entry.name())?;
        if !paths.insert(path.clone()) {
            return Err(ReleaseError::DuplicateArchivePath(path));
        }
        if entry.is_symlink() {
            return Err(ReleaseError::UnsupportedArchiveEntry(
                entry.name().to_owned(),
            ));
        }
        if path == format!("{PACKAGE_DIRECTORY}/{TOC_NAME}") {
            toc = true;
        }
    }
    if !toc {
        return Err(ReleaseError::MissingToc);
    }
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(ReleaseError::OpenArchive)?;
        let path = canonical_archive_path(entry.name())?;
        let target = destination.join(path);
        if entry.is_dir() {
            fs::create_dir_all(&target).map_err(|source| ReleaseError::Extract {
                path: target,
                source,
            })?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|source| ReleaseError::Extract {
                    path: parent.to_path_buf(),
                    source,
                })?;
            }
            let mut output = File::create(&target).map_err(|source| ReleaseError::Extract {
                path: target.clone(),
                source,
            })?;
            io::copy(&mut entry, &mut output).map_err(|source| ReleaseError::Extract {
                path: target,
                source,
            })?;
        }
    }
    Ok(())
}

fn canonical_archive_path(raw: &str) -> Result<String, ReleaseError> {
    if raw.starts_with('/') || raw.starts_with('\\') || raw.as_bytes().get(1) == Some(&b':') {
        return Err(ReleaseError::UnsafeArchivePath(raw.to_owned()));
    }
    let replaced = raw.replace('\\', "/");
    let mut pieces = Vec::new();
    for part in replaced.split('/') {
        if part.is_empty() {
            continue;
        }
        if part == "." || part == ".." || part.contains(':') {
            return Err(ReleaseError::UnsafeArchivePath(raw.to_owned()));
        }
        pieces.push(part);
    }
    if pieces.first() != Some(&PACKAGE_DIRECTORY) {
        return Err(ReleaseError::UnsafeArchivePath(raw.to_owned()));
    }
    let path = Path::new(&replaced);
    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(ReleaseError::UnsafeArchivePath(raw.to_owned()));
    }
    Ok(pieces.join("/"))
}

#[cfg(test)]
mod tests {
    use std::{fs, io::Write};

    use zip::{write::FileOptions, ZipWriter};

    use super::*;

    fn asset(name: String) -> GitHubAsset {
        GitHubAsset {
            name,
            browser_download_url: "https://example.invalid/download".to_owned(),
            digest: None,
        }
    }

    fn release() -> GitHubRelease {
        let version = "2.0.alpha5";
        GitHubRelease {
            tag_name: version.to_owned(),
            name: None,
            published_at: None,
            assets: vec![
                asset(format!("RPEngine2-{version}.zip")),
                asset(format!("RPEngine2-{version}.zip.sha256")),
            ],
        }
    }

    fn archive(entries: &[(&str, &str)]) -> Vec<u8> {
        let cursor = Cursor::new(Vec::new());
        let mut writer = ZipWriter::new(cursor);
        let options: FileOptions<'_, ()> = FileOptions::default();
        for (name, contents) in entries {
            writer
                .start_file(name, options)
                .expect("start test ZIP entry");
            writer
                .write_all(contents.as_bytes())
                .expect("write test ZIP entry");
        }
        writer.finish().expect("finish test ZIP").into_inner()
    }

    fn checksum(zip: &[u8]) -> Vec<u8> {
        format!("{}  RPEngine2-2.0.alpha5.zip\n", hex_digest(zip)).into_bytes()
    }

    fn temporary_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("rpengine-release-{name}-{nonce}"));
        fs::create_dir_all(&root).expect("create temporary root");
        root
    }

    fn stage(entries: &[(&str, &str)]) -> Result<StagedRelease, ReleaseError> {
        let zip = archive(entries);
        let root = temporary_root("stage");
        let result = stage_release_bytes(&release(), &zip, &checksum(&zip), None, &root);
        if result.is_err() {
            fs::remove_dir_all(root).expect("remove failed staging root");
        }
        result
    }

    #[test]
    fn selects_only_the_canonical_release_assets() {
        let release = release();
        let assets = select_release_assets(&release).expect("select assets");
        assert_eq!(assets.zip.name, "RPEngine2-2.0.alpha5.zip");
        assert_eq!(assets.checksum.name, "RPEngine2-2.0.alpha5.zip.sha256");
    }

    #[test]
    fn rejects_missing_or_duplicate_release_assets() {
        let mut missing_zip = release();
        missing_zip.assets.remove(0);
        assert!(matches!(
            select_release_assets(&missing_zip),
            Err(ReleaseError::MissingAsset("ZIP"))
        ));
        let mut missing_checksum = release();
        missing_checksum.assets.pop();
        assert!(matches!(
            select_release_assets(&missing_checksum),
            Err(ReleaseError::MissingAsset("checksum"))
        ));
        let mut duplicate = release();
        duplicate
            .assets
            .push(asset("RPEngine2-2.0.alpha5.zip".to_owned()));
        assert!(matches!(
            select_release_assets(&duplicate),
            Err(ReleaseError::DuplicateAsset(_))
        ));
        let mut malformed = release();
        malformed.assets[0].browser_download_url = "not a URL".to_owned();
        assert!(matches!(
            select_release_assets(&malformed),
            Err(ReleaseError::MalformedAsset(_))
        ));
    }

    #[test]
    fn verifies_checksum_and_optional_github_digest() {
        let package = b"verified package";
        let digest = hex_digest(package);
        assert!(verify_package(package, &digest, Some(&format!("sha256:{digest}"))).is_ok());
        assert!(matches!(
            verify_package(package, &"0".repeat(64), None),
            Err(ReleaseError::ChecksumMismatch)
        ));
        assert!(matches!(
            verify_package(
                package,
                &digest,
                Some(&format!("sha256:{}", "0".repeat(64)))
            ),
            Err(ReleaseError::GitHubDigestMismatch)
        ));
        assert!(matches!(
            parse_checksum("not a checksum", "2.0.alpha5"),
            Err(ReleaseError::MalformedChecksum)
        ));
    }

    #[test]
    fn stages_a_valid_canonical_archive() {
        let staged = stage(&[
            ("RPEngine2/RPEngine2.toc", "## Version: 2.0.alpha5\n"),
            ("RPEngine2/Core.lua", "return {}"),
        ])
        .expect("stage release");
        assert_eq!(staged.version, "2.0.alpha5");
        assert!(staged.directory.join("RPEngine2/Core.lua").is_file());
        fs::remove_dir_all(staged.directory.parent().expect("staging root"))
            .expect("remove staging root");
    }

    #[test]
    fn rejects_unsafe_and_noncanonical_archives() {
        for entries in [
            vec![("/RPEngine2/RPEngine2.toc", "## Version: 2.0.alpha5")],
            vec![
                ("RPEngine2/../outside", "x"),
                ("RPEngine2/RPEngine2.toc", "## Version: 2.0.alpha5"),
            ],
            vec![("Other/RPEngine2.toc", "## Version: 2.0.alpha5")],
            vec![
                ("outside.txt", "x"),
                ("RPEngine2/RPEngine2.toc", "## Version: 2.0.alpha5"),
            ],
        ] {
            assert!(matches!(
                stage(&entries),
                Err(ReleaseError::UnsafeArchivePath(_))
            ));
        }
        assert!(matches!(
            stage(&[("RPEngine2/other.lua", "x")]),
            Err(ReleaseError::MissingToc)
        ));
        assert!(matches!(
            stage(&[
                ("RPEngine2/RPEngine2.toc", "## Version: 2.0.alpha5"),
                ("RPEngine2\\RPEngine2.toc", "duplicate"),
            ]),
            Err(ReleaseError::DuplicateArchivePath(_))
        ));
    }

    #[test]
    fn rejects_staged_toc_version_mismatch() {
        assert!(matches!(
            stage(&[("RPEngine2/RPEngine2.toc", "## Version: 2.0.alpha4")]),
            Err(ReleaseError::TocVersionMismatch { .. })
        ));
    }
}
