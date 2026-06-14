use std::collections::HashMap;

pub fn qualify(sector: &str, name: &str) -> String {
    format!("{sector}.{name}")
}

pub fn is_qualified(name: &str) -> bool {
    name.contains('.')
}

pub fn resolve_symbol<T>(
    name: &str,
    current_sector: Option<&str>,
    table: &HashMap<String, T>,
) -> Option<String> {
    if table.contains_key(name) {
        return Some(name.to_string());
    }

    if !is_qualified(name) {
        if let Some(sector) = current_sector {
            let qualified = qualify(sector, name);
            if table.contains_key(&qualified) {
                return Some(qualified);
            }
        }
    }

    None
}