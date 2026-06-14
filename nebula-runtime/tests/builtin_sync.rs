use nebula_builtins::manifest;

#[test]
fn manifest_names_are_registered_builtins() {
    for name in manifest().names() {
        assert!(nebula_builtins::is_builtin(name), "missing builtin registration for {name}");
    }
}