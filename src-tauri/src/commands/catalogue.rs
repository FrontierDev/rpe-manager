//! Narrow Tauri read commands; the frontend never receives HTTP details.

use serde::Serialize;

use crate::catalogue::{
    CatalogueClient, CatalogueError, CataloguePackagesResponse, PackageDetailResponse,
    PublishersResponse,
};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogueCommandError {
    pub code: String,
    pub message: String,
}
impl From<CatalogueError> for CatalogueCommandError {
    fn from(error: CatalogueError) -> Self {
        Self {
            code: error.code().into(),
            message: error.to_string(),
        }
    }
}

#[tauri::command]
pub fn get_catalogue_packages(
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<CataloguePackagesResponse, CatalogueCommandError> {
    CatalogueClient::production()?
        .get_catalogue_packages(limit, offset)
        .map_err(Into::into)
}
#[tauri::command]
pub fn get_catalogue_package(
    catalogue_id: String,
) -> Result<PackageDetailResponse, CatalogueCommandError> {
    CatalogueClient::production()?
        .get_package(&catalogue_id)
        .map_err(Into::into)
}
#[tauri::command]
pub fn get_catalogue_publishers() -> Result<PublishersResponse, CatalogueCommandError> {
    CatalogueClient::production()?
        .get_publishers()
        .map_err(Into::into)
}
