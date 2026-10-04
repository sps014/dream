//! Plain export signatures persisted for foreign-language bridge generation.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportKind {
    Void,
    Int,
    UInt,
    Long,
    ULong,
    Bool,
    Byte,
    Char,
    ISize,
    USize,
    Float,
    Double,
    Opaque,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportType {
    pub c_type: String,
    pub kind: ExportKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportParam {
    pub ty: ExportType,
    pub take: bool,
    pub is_ref: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportFunction {
    pub name: String,
    pub ret: ExportType,
    pub params: Vec<ExportParam>,
}

pub fn to_json(functions: &[ExportFunction]) -> String {
    serde_json::to_string(functions).expect("export signatures serialize")
}
