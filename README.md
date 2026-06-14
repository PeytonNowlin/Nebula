# Nebula

Nebula is a general-purpose programming language designed for AI agent authors. Every construct favors machine parseability over human brevity: operators are spelled as keywords (`plus`, `eq`, `less than`), types are always explicit, and the toolchain exposes structured error codes for reliable agent feedback.

This repository contains the Nebula compiler and interpreter, implemented in Rust.

## Features

- **Keyword-based syntax** — arithmetic and comparisons use words instead of symbols, so parsers and agents can read source without ambiguity.
- **Sectors** — modular namespaces for functions, structs, and probes (`math.double`, `geo.Point`).
- **Probes** — declare external capabilities in source; the runtime logs probe invocations for host integration.
- **Telemetry** — `telemetry` blocks emit structured JSONL traces for each statement executed inside them.
- **Imports** — compose programs from library modules with cycle and duplicate detection.
- **Full pipeline** — parse → load/merge → typecheck → IR → interpret, with `check`, `fmt`, and `run` CLI commands.

## Requirements

- [Rust](https://www.rust-lang.org/) (2021 edition; any recent stable toolchain)

## Quick start

```bash
# Build the CLI
cargo build --release

# Run an example
cargo run -- run examples/hello.neb

# Or use the binary directly after building
./target/release/nebula run examples/hello.neb
```

Expected output:

```
Hello from Nebula
```

## CLI

The `nebula` binary provides three subcommands:

| Command | Description |
|---------|-------------|
| `nebula check <file>` | Parse, resolve imports, and typecheck without running |
| `nebula fmt <file>` | Print canonical formatted source |
| `nebula fmt <file> --write` | Rewrite the file in place |
| `nebula run <file>` | Typecheck and execute via the interpreter |
| `nebula run <file> --telemetry <path>` | Run with JSONL telemetry written to `<path>` |

```bash
cargo run -- check examples/fizzbuzz.neb
cargo run -- fmt examples/hello.neb
cargo run -- run examples/agent_counter.neb --telemetry trace.jsonl
```

## Language overview

Programs consist of top-level `sector` declarations (libraries), `import` statements, and a single `mission main` entry point.

```nebula
sector math {
  fn double(n: Int) -> Int {
    return n times 2;
  }
}

mission main {
  let mut i: Int = 1;

  while i le 20 do {
    print(math.double(i));
    set i = i plus 1;
  }
}
```

### Types

`Int`, `Float`, `Bool`, `Str`, `Void`, `List<T>`, `Map<K, V>`, `Option<T>`, and function types `fn(T1, T2) -> R`. All bindings, parameters, and returns require explicit annotations.

### Builtins

Provided by the runtime (see `std/core.neb`):

| Function | Signature |
|----------|-----------|
| `print` | `fn(value: Str) -> Void` |
| `len` | `fn(value: List<T>) -> Int` |
| `push` | `fn(list: List<T>, value: T) -> Void` |
| `str_to_int` | `fn(s: Str) -> Int` |
| `int_to_str` | `fn(n: Int) -> Str` |

### Probes and telemetry

Probes declare host-provided capabilities. `call` invokes them at runtime (currently logged to stdout).

```nebula
mission main {
  probe log(level: Str, message: Str) -> Void;

  telemetry {
    call log(level: "info", message: "starting");
  }
}
```

With `--telemetry`, each statement inside a `telemetry` block appends a JSONL event describing the step.

### Imports

```nebula
import "../std/math.neb";

mission main {
  print(int_to_str(math.triple(7)));
}
```

Import paths are relative to the importing file. Library modules may contain sectors and nested imports but must not define a `mission`.

### Error codes

| Prefix | Category |
|--------|----------|
| `NEB-S` | Syntax / parse |
| `NEB-T` | Type |
| `NEB-R` | Runtime |
| `NEB-P` | Probe |
| `NEB-L` | Module load / import |

## Examples

| File | Demonstrates |
|------|--------------|
| `examples/hello.neb` | Minimal program |
| `examples/fizzbuzz.neb` | Sectors, conditionals, loops |
| `examples/end_demo.neb` | `end`-style blocks (alternative to braces) |
| `examples/push_demo.neb` | Lists and builtins |
| `examples/import_demo.neb` | Standard library imports |
| `examples/agent_counter.neb` | Probes, telemetry, mutable state |

## Project structure

Rust workspace crates, each handling one stage of the pipeline:

| Crate | Role |
|-------|------|
| `nebula-syntax` | Lexer and parser (logos + chumsky) |
| `nebula-ast` | Abstract syntax tree types |
| `nebula-load` | Import resolution and program merging |
| `nebula-types` | Type checker |
| `nebula-ir` | Intermediate representation lowering |
| `nebula-runtime` | Tree-walking interpreter |
| `nebula-fmt` | Canonical formatter |
| `nebula-cli` | `nebula` command-line tool |

The language specification lives in [`nebula-spec/SPEC.md`](nebula-spec/SPEC.md). Shared library modules are under `std/`.

## Development

```bash
# Run all tests
cargo test

# Format Rust code
cargo fmt

# Lint
cargo clippy
```

## License

MIT — see [LICENSE](LICENSE).
