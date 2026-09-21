//! Typed, read-only client for the public Esarus RPE catalogue API.

mod client;
mod models;

pub use client::{CatalogueClient, CatalogueError, DEFAULT_BASE_URL};
pub use models::*;
