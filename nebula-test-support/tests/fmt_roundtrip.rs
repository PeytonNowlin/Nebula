use std::fs;

use nebula_test_support::{assert_golden, fmt_roundtrip, workspace_root};

#[test]
fn fmt_is_idempotent_on_messy_input() {
    let src = r#"mission main{
let mut count:Int=0;
while count less than 3 do{print(int_to_str(count));set count=count plus 1;}
if count eq 3 then{print("done");}else{print("unexpected");}
}"#;
    fmt_roundtrip(src);
}

#[test]
fn golden_fmt_end_demo_canonicalizes_to_end_blocks() {
    let path = workspace_root().join("examples/end_demo.neb");
    let source = fs::read_to_string(&path).expect("read end_demo");
    let formatted = fmt_roundtrip(&source);
    assert_golden("fmt", "end_demo", &formatted);
}