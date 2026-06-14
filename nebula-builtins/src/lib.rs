use std::sync::OnceLock;

use nebula_ast::Type;
use serde::Deserialize;

const MANIFEST_TOML: &str = include_str!("../builtins.toml");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinCheckerKind {
    Simple,
    Len,
    Push,
    At,
    Get,
    Has,
}

#[derive(Debug, Clone)]
pub struct BuiltinDef {
    pub name: String,
    pub params: Vec<(String, Type)>,
    pub return_type: Type,
    pub checker: BuiltinCheckerKind,
    pub python_name: String,
}

#[derive(Debug)]
pub struct BuiltinManifest {
    builtins: Vec<BuiltinDef>,
    by_name: std::collections::HashMap<String, usize>,
}

impl BuiltinManifest {
    pub fn builtins(&self) -> &[BuiltinDef] {
        &self.builtins
    }

    pub fn get(&self, name: &str) -> Option<&BuiltinDef> {
        self.by_name.get(name).map(|&idx| &self.builtins[idx])
    }

    pub fn is_builtin(&self, name: &str) -> bool {
        self.by_name.contains_key(name)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.builtins.iter().map(|builtin| builtin.name.as_str())
    }

    pub fn simple_signatures(&self) -> Vec<(&str, Vec<(String, Type)>, Type)> {
        self.builtins
            .iter()
            .filter(|builtin| builtin.checker == BuiltinCheckerKind::Simple)
            .map(|builtin| {
                (
                    builtin.name.as_str(),
                    builtin.params.clone(),
                    builtin.return_type.clone(),
                )
            })
            .collect()
    }
}

pub fn manifest() -> &'static BuiltinManifest {
    static MANIFEST: OnceLock<BuiltinManifest> = OnceLock::new();
    MANIFEST.get_or_init(|| parse_manifest(MANIFEST_TOML).expect("invalid builtins.toml"))
}

pub fn is_builtin(name: &str) -> bool {
    manifest().is_builtin(name)
}

fn parse_manifest(source: &str) -> Result<BuiltinManifest, String> {
    let raw: RawManifest =
        toml::from_str(source).map_err(|err| format!("failed to parse builtins.toml: {err}"))?;

    let mut builtins = Vec::with_capacity(raw.builtin.len());
    let mut by_name = std::collections::HashMap::new();

    for entry in raw.builtin {
        let checker = parse_checker(entry.checker.as_deref(), &entry.params)?;
        let params = entry
            .params
            .iter()
            .map(|param| {
                Ok((
                    param.name.clone(),
                    parse_type(&param.ty).map_err(|err| format!("{}: {err}", entry.name))?,
                ))
            })
            .collect::<Result<Vec<_>, String>>()?;

        let return_type = match entry.returns.as_deref() {
            Some(text) => parse_type(text).map_err(|err| format!("{}: {err}", entry.name))?,
            None if matches!(checker, BuiltinCheckerKind::At | BuiltinCheckerKind::Get) => {
                Type::Void
            }
            None => {
                return Err(format!(
                    "{}: `returns` is required unless checker is `at` or `get`",
                    entry.name
                ));
            }
        };

        if checker == BuiltinCheckerKind::Simple && params.is_empty() {
            return Err(format!("{}: simple builtin requires params", entry.name));
        }

        let python_name = entry
            .python
            .unwrap_or_else(|| format!("nebula_{}", entry.name));

        let idx = builtins.len();
        if by_name.insert(entry.name.clone(), idx).is_some() {
            return Err(format!("duplicate builtin `{}`", entry.name));
        }

        builtins.push(BuiltinDef {
            name: entry.name,
            params,
            return_type,
            checker,
            python_name,
        });
    }

    Ok(BuiltinManifest { builtins, by_name })
}

fn parse_checker(
    checker: Option<&str>,
    params: &[RawParam],
) -> Result<BuiltinCheckerKind, String> {
    match checker {
        None if !params.is_empty() => Ok(BuiltinCheckerKind::Simple),
        None => Err("non-simple builtin requires `checker`".into()),
        Some("len") => Ok(BuiltinCheckerKind::Len),
        Some("push") => Ok(BuiltinCheckerKind::Push),
        Some("at") => Ok(BuiltinCheckerKind::At),
        Some("get") => Ok(BuiltinCheckerKind::Get),
        Some("has") => Ok(BuiltinCheckerKind::Has),
        Some(other) => Err(format!("unknown builtin checker `{other}`")),
    }
}

fn parse_type(text: &str) -> Result<Type, String> {
    match text {
        "Int" => Ok(Type::Int),
        "Float" => Ok(Type::Float),
        "Bool" => Ok(Type::Bool),
        "Str" => Ok(Type::Str),
        "Void" => Ok(Type::Void),
        other => Err(format!("unsupported manifest type `{other}`")),
    }
}

#[derive(Debug, Deserialize)]
struct RawManifest {
    builtin: Vec<RawBuiltin>,
}

#[derive(Debug, Deserialize)]
struct RawBuiltin {
    name: String,
    #[serde(default)]
    params: Vec<RawParam>,
    returns: Option<String>,
    checker: Option<String>,
    python: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawParam {
    name: String,
    #[serde(rename = "type")]
    ty: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_lists_all_runtime_builtins() {
        let names: Vec<_> = manifest().names().collect();
        assert_eq!(names.len(), 12);
        for name in [
            "print", "len", "push", "at", "get", "has", "str_to_int", "int_to_str",
            "str_to_float", "float_to_str", "int_to_float", "float_to_int",
        ] {
            assert!(manifest().is_builtin(name), "missing builtin {name}");
        }
    }

    #[test]
    fn simple_signatures_cover_conversion_builtins() {
        let simple: std::collections::HashSet<_> = manifest()
            .simple_signatures()
            .into_iter()
            .map(|(name, _, _)| name)
            .collect();
        for name in ["print", "str_to_int", "int_to_str", "float_to_int"] {
            assert!(simple.contains(name), "missing simple builtin {name}");
        }
    }
}