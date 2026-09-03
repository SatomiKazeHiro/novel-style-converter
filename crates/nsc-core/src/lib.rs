pub mod error;
pub mod models;
pub mod db;
pub mod ai;
pub mod splitter;
pub mod encoding;
pub mod text;
pub mod transformer;
pub mod upload;
pub mod prompts;
pub mod cleaner;
pub mod startup_cleanup;
pub mod startup_recovery;
pub mod recorder;
pub mod catalog;

pub use error::{Error, Result};
pub use catalog::{CatalogMeta, CatalogOrigin, CatalogSource, CatalogStatus, CatalogStore};
