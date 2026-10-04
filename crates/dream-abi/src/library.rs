//! Manifest settings shared by the compiler and package manager.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LibraryKind {
    #[serde(rename = "staticlib")]
    Staticlib,
    #[serde(rename = "cdylib")]
    Cdylib,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LibraryConfig {
    #[serde(rename = "output-type")]
    pub output_type: LibraryKind,
}
