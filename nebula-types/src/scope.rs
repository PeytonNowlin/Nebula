use std::collections::HashMap;

use nebula_ast::Type;

pub(crate) struct Scope {
    bindings: HashMap<String, (Type, bool)>,
}

impl Scope {
    pub(crate) fn new() -> Self {
        Self {
            bindings: HashMap::new(),
        }
    }

    pub(crate) fn define(&mut self, name: String, ty: Type, mutable: bool) {
        self.bindings.insert(name, (ty, mutable));
    }

    pub(crate) fn get(&self, name: &str) -> Option<&(Type, bool)> {
        self.bindings.get(name)
    }
}
