//! Official RPE2 release metadata and version ordering.

use std::{cmp::Ordering, fmt};

use serde::Deserialize;

pub const LATEST_RELEASE_URL: &str = "https://api.github.com/repos/FrontierDev/rpe2/releases/latest";

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
enum VersionChannel { Alpha, Final }

impl RpeVersion {
    pub fn parse(value: &str) -> Result<Self, VersionError> {
        let parts: Vec<_> = value.split('.').collect();
        let [major, minor, channel] = parts.as_slice() else { return Err(VersionError(value.to_owned())); };
        let (kind, number) = if let Some(number) = channel.strip_prefix("alpha") {
            (VersionChannel::Alpha, number.parse().ok())
        } else if channel.is_empty() { (VersionChannel::Final, Some(0)) } else { (VersionChannel::Final, None) };
        Ok(Self { major: major.parse().map_err(|_| VersionError(value.to_owned()))?, minor: minor.parse().map_err(|_| VersionError(value.to_owned()))?, channel: kind, channel_number: number.ok_or_else(|| VersionError(value.to_owned()))? })
    }
}

impl Ord for RpeVersion {
    fn cmp(&self, other: &Self) -> Ordering { (self.major, self.minor, self.channel, self.channel_number).cmp(&(other.major, other.minor, other.channel, other.channel_number)) }
}
impl PartialOrd for RpeVersion { fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) } }

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VersionError(String);
impl fmt::Display for VersionError { fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "unsupported RPEngine version {:?}", self.0) } }
impl std::error::Error for VersionError {}

pub fn fetch_latest_release() -> Result<GitHubRelease, String> {
    reqwest::blocking::Client::builder().user_agent("RPEngine-Manager").build().map_err(|e| e.to_string())?
        .get(LATEST_RELEASE_URL).send().map_err(|e| e.to_string())?.error_for_status().map_err(|e| e.to_string())?
        .json().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests { use super::*;
    #[test] fn orders_versions() { assert_eq!(RpeVersion::parse("2.0.alpha4").unwrap().cmp(&RpeVersion::parse("2.0.alpha5").unwrap()), Ordering::Less); assert!(RpeVersion::parse("2.0.alpha5").unwrap() > RpeVersion::parse("2.0.alpha4").unwrap()); assert!(RpeVersion::parse("bad").is_err()); }
}
