use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pagination {
    pub limit: u32,
    pub offset: u32,
    pub has_more: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Compatibility {
    pub minimum_rpe_version: Option<String>,
    pub maximum_rpe_version: Option<String>,
    pub required_manager_protocol_version: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencySummary {
    pub catalogue_id: String,
    #[serde(default)]
    pub package_type: Option<String>,
    #[serde(default)]
    pub native_rpe_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub minimum_revision: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CataloguePackage {
    pub catalogue_id: String,
    pub publisher_id: String,
    pub publisher_type: String,
    pub publisher_display_name: String,
    pub package_type: String,
    /// Dataset ID for datasets, and the native ruleset ID for rulesets.  This
    /// deliberately remains distinct from both `catalogue_id` and `revision`.
    pub native_rpe_id: String,
    pub dataset_type: Option<String>,
    pub dataset_group: Option<String>,
    pub name: String,
    pub short_description: String,
    pub full_description: String,
    pub current_revision: u64,
    pub publication_state: String,
    pub protected: bool,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub compatibility: Option<Compatibility>,
    #[serde(default)]
    pub dependencies: Vec<DependencySummary>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Revision {
    pub catalogue_id: String,
    pub revision: u64,
    pub sha256: String,
    pub payload_byte_size: u64,
    pub changelog: String,
    pub published_at: String,
    pub minimum_rpe_version: Option<String>,
    pub maximum_rpe_version: Option<String>,
    pub required_manager_protocol_version: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevisionWithDependencies {
    pub revision: Revision,
    #[serde(default)]
    pub dependencies: Vec<DependencySummary>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CataloguePackagesResponse {
    pub packages: Vec<CataloguePackage>,
    pub pagination: Pagination,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageDetailResponse {
    pub package: CataloguePackage,
    pub current_revision: Revision,
    pub revision_history: Vec<Revision>,
    #[serde(default)]
    pub dependencies: Vec<DependencySummary>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevisionsResponse {
    pub package: CataloguePackage,
    pub current_revision: u64,
    pub revisions: Vec<RevisionWithDependencies>,
    pub pagination: Pagination,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevisionDetailResponse {
    pub package: CataloguePackage,
    pub revision: Revision,
    #[serde(default)]
    pub dependencies: Vec<DependencySummary>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Publisher {
    pub publisher_id: String,
    pub publisher_type: String,
    pub display_name: String,
    pub description: Option<String>,
    pub icon_url: Option<String>,
    pub image_url: Option<String>,
    pub visibility: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PublishersResponse {
    pub publishers: Vec<Publisher>,
}
