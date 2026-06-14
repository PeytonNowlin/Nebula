use std::collections::HashMap;
use std::env;

use serde::Deserialize;

use crate::RuntimeError;

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum SecretBinding {
    Env { env: String },
    Value { value: String },
}

pub type SecretsStore = HashMap<String, String>;

pub fn resolve_secrets(
    bindings: &HashMap<String, SecretBinding>,
    overlay: Option<&SecretsStore>,
) -> Result<SecretsStore, RuntimeError> {
    let mut store = SecretsStore::new();
    for (name, binding) in bindings {
        let value = match binding {
            SecretBinding::Env { env } => env::var(env).map_err(|_| RuntimeError::Error {
                message: format!(
                    "secret `{name}` references unset environment variable `{env}`"
                ),
            })?,
            SecretBinding::Value { value } => value.clone(),
        };
        store.insert(name.clone(), value);
    }
    if let Some(overlay) = overlay {
        for (name, value) in overlay {
            store.insert(name.clone(), value.clone());
        }
    }
    Ok(store)
}

pub fn substitute_secrets(template: &str, store: &SecretsStore) -> Result<String, RuntimeError> {
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find("${secret:") {
        out.push_str(&rest[..start]);
        let after = &rest[start + "${secret:".len()..];
        let Some(end) = after.find('}') else {
            return Err(RuntimeError::Error {
                message: format!("unclosed secret template in `{template}`"),
            });
        };
        let name = &after[..end];
        let value = store.get(name).ok_or_else(|| RuntimeError::Error {
            message: format!("unknown secret `{name}` in template `{template}`"),
        })?;
        out.push_str(value);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

pub fn substitute_string_map(
    map: &mut HashMap<String, String>,
    store: &SecretsStore,
) -> Result<(), RuntimeError> {
    for value in map.values_mut() {
        *value = substitute_secrets(value, store)?;
    }
    Ok(())
}

pub fn substitute_string_vec(values: &mut [String], store: &SecretsStore) -> Result<(), RuntimeError> {
    for value in values.iter_mut() {
        *value = substitute_secrets(value, store)?;
    }
    Ok(())
}