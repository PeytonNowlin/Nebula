use std::collections::HashMap;

use nebula_ast::{Program, Type};

#[derive(Debug, Clone)]
pub struct TypedProgram {
    pub program: Program,
    pub functions: HashMap<String, FnInfo>,
    pub structs: HashMap<String, StructInfo>,
    pub probes: HashMap<String, ProbeInfo>,
    pub has_main: bool,
}

#[derive(Debug, Clone)]
pub struct FnInfo {
    pub sector: String,
    pub name: String,
    pub qualified_name: String,
    pub params: Vec<(String, Type)>,
    pub return_type: Type,
}

#[derive(Debug, Clone)]
pub struct StructInfo {
    pub sector: String,
    pub name: String,
    pub qualified_name: String,
    pub fields: HashMap<String, Type>,
}

#[derive(Debug, Clone)]
pub struct ProbeInfo {
    pub sector: Option<String>,
    pub name: String,
    pub qualified_name: String,
    pub params: Vec<(String, Type)>,
    pub return_type: Type,
}
