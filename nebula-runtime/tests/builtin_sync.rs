use std::collections::HashMap;

use nebula_builtins::{manifest, BuiltinCheckerKind};
use nebula_ir::{IrMission, IrProgram};
use nebula_runtime::{missing_runtime_handlers, Runtime, RuntimeError};

fn empty_program() -> IrProgram {
    IrProgram {
        sectors: HashMap::new(),
        mission: IrMission {
            name: "main".into(),
            stmts: Vec::new(),
        },
        probes: HashMap::new(),
    }
}

#[test]
fn manifest_builtins_have_static_runtime_handlers() {
    let missing = missing_runtime_handlers();
    assert!(
        missing.is_empty(),
        "builtins.toml entries without runtime handlers: {missing:?}"
    );
}

#[test]
fn manifest_simple_signatures_match_handler_table() {
    let simple_names: Vec<_> = manifest()
        .simple_signatures()
        .into_iter()
        .map(|(name, _, _)| name)
        .collect();
    assert_eq!(simple_names.len(), 7);
    for name in simple_names {
        let def = manifest().get(name).expect("simple builtin");
        assert_eq!(def.checker, BuiltinCheckerKind::Simple);
    }
}

#[test]
fn dispatch_never_hits_missing_handler_arm() {
    let mut rt = Runtime::new(&empty_program());
    for name in manifest().names() {
        let result = rt.eval_builtin_for_coverage(name);
        if let Err(RuntimeError::Error { message }) = result {
            assert!(
                !message.contains("listed in builtins.toml but has no runtime handler"),
                "builtin `{name}` is missing a runtime handler"
            );
        }
    }
}