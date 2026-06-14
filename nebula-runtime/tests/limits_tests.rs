use std::time::Duration;

use nebula_ir::lower;
use nebula_runtime::{ResourceLimits, Runtime};
use nebula_syntax::parse;
use nebula_types::typecheck;

fn run_with_limits(src: &str, limits: ResourceLimits) -> Result<(), String> {
    let program = parse(src).map_err(|e| e.to_string())?;
    let typed = typecheck(&program).map_err(|e| e.to_string())?;
    let ir = lower(&typed).map_err(|e| e.to_string())?;
    let mut runtime = Runtime::new(&ir).with_resource_limits(limits);
    runtime.run(&ir).map(|_| ()).map_err(|e| e.to_string())
}

#[test]
fn unlimited_runtime_runs_hello() {
    let src = r#"
mission main {
  print("hi");
}
"#;
    run_with_limits(src, ResourceLimits::unlimited()).expect("hello should run");
}

#[test]
fn loop_iteration_limit_traps_infinite_while() {
    let src = r#"
mission main {
  while true eq true do
    print("spin");
  end
}
"#;
    let err = run_with_limits(
        src,
        ResourceLimits {
            max_runtime: None,
            max_loop_iterations: Some(10),
            max_memory_bytes: None,
        },
    )
    .expect_err("infinite loop should fail");
    assert!(err.contains("NEB-R009"), "err: {err}");
}

#[test]
fn execution_timeout_traps_long_running_program() {
    let src = r#"
mission main {
  let mut i: Int = 0;
  while i lt 1000000000 do
    set i = i plus 1;
  end
}
"#;
    let err = run_with_limits(
        src,
        ResourceLimits {
            max_runtime: Some(Duration::from_millis(50)),
            max_loop_iterations: None,
            max_memory_bytes: None,
        },
    )
    .expect_err("long run should time out");
    assert!(err.contains("NEB-R008"), "err: {err}");
}

#[test]
fn memory_limit_traps_growing_list() {
    let src = r#"
mission main {
  let mut xs: List<Str> = [];
  while true eq true do
    push(xs, "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx");
  end
}
"#;
    let err = run_with_limits(
        src,
        ResourceLimits {
            max_runtime: None,
            max_loop_iterations: None,
            max_memory_bytes: Some(4096),
        },
    )
    .expect_err("growing list should exceed memory");
    assert!(err.contains("NEB-R010"), "err: {err}");
}

#[test]
fn default_runtime_new_has_no_limits() {
    let src = r#"
mission main {
  let mut i: Int = 0;
  while i lt 100 do
    set i = i plus 1;
  end
}
"#;
    let program = parse(src).expect("parse");
    let typed = typecheck(&program).expect("typecheck");
    let ir = lower(&typed).expect("lower");
    let mut runtime = Runtime::new(&ir);
    runtime.run(&ir).expect("default runtime should be unlimited");
}

