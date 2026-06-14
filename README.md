# Nebula

Nebula is a general-purpose programming language designed for AI agent authors. Every construct favors machine parseability over human brevity: operators are spelled as keywords (`plus`, `eq`, `lt`), types are always explicit, and the toolchain exposes structured error codes and JSON output for reliable agent feedback.

This repository contains the Nebula compiler and interpreter, implemented in Rust.

## Features

- **Keyword-based syntax** — arithmetic and comparisons use words instead of symbols, so parsers and agents can read source without ambiguity.
- **Sectors** — modular namespaces for functions, structs, and probes (`math.double`, `geo.Point`).
- **Probes** — declare external capabilities in source; `call` dispatches them through a probe host (`jsonl`, external `command`, or **MCP** via `--probes` manifest).
- **Telemetry** — `telemetry` blocks emit structured JSONL traces for each statement executed inside them.
- **Imports** — compose programs from library modules with cycle and duplicate detection (`NEB-L003`, `NEB-T009`).
- **Agent-oriented tooling** — GBNF grammar at [`grammar/nebula.gbnf`](grammar/nebula.gbnf) for constrained LLM code generation; deprecated syntax forms are rejected at parse time (`NEB-S004`, `NEB-S005`).
- **Structured JSON I/O** — `check`, `run`, `parse`, `ir`, and `probes list` support `--json` for machine-readable diagnostics, AST export, IR export, and probe introspection.
- **In-process embedding** — the `nebula-host` crate exposes `check_source`, `run_source`, and `list_probes` for agent runtimes that want to avoid subprocess overhead.
- **Full pipeline** — parse → load/merge → typecheck → IR → interpret or transpile.

## Requirements

- [Rust](https://www.rust-lang.org/) (2021 edition; any recent stable toolchain)
- [Python 3](https://www.python.org/) (required for MCP probe servers, transpiled output, and integration tests)

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

The `nebula` binary provides seven subcommands: `check`, `parse`, `ir`, `fmt`, `run`, `probes`, and `compile`.

### Validation and execution

| Command | Description |
|---------|-------------|
| `nebula check <file>` | Parse, resolve imports, and typecheck without running |
| `nebula check <file> --json` | On success, print `[]` to stdout; on failure, print a JSON diagnostic array to stderr |
| `nebula run <file>` | Typecheck and execute via the interpreter |
| `nebula run <file> --json` | Same as `check --json` on failure; stdout is reserved for program output when successful |
| `nebula run <file> --telemetry <path>` | Run with JSONL telemetry written to `<path>` |
| `nebula run <file> --probes <path>` | Load a probe host manifest (JSON) for custom probe handlers |

### Introspection and export

| Command | Description |
|---------|-------------|
| `nebula parse <file> --json` | Export the parsed AST as JSON on stdout |
| `nebula parse <file> --json --load` | Resolve imports and export the merged workspace AST |
| `nebula ir <file> --json` | Typecheck, lower, and export IR as JSON on stdout |
| `nebula probes list --probes <path>` | List manifest probe bindings |
| `nebula probes list --probes <path> --mcp` | Also query each MCP server's live `tools/list` catalog |
| `nebula probes list --probes <path> --json [--mcp]` | Emit the probe report as JSON on stdout |

### Formatting and transpilation

| Command | Description |
|---------|-------------|
| `nebula fmt <file>` | Parse, resolve imports, and print canonical formatted entry file |
| `nebula fmt <file> --write` | Format the entry file and every imported module in place |
| `nebula compile <file> --target python --out <dir>` | Transpile to a multi-module Python package |
| `nebula compile <file> --target python --out <dir> --probes <path>` | Embed probe manifest defaults in the entry module |

### JSON diagnostic format

When `--json` is passed to `check` or `run`, failures emit a JSON array of diagnostic objects on **stderr**:

```json
[
  {
    "code": "NEB-T002",
    "span": {
      "file": "example.neb",
      "start": 42,
      "end": 58,
      "line": 3,
      "column": 15
    },
    "message": "type mismatch: expected Int, found Str"
  }
]
```

Each record has `code`, `message`, and an optional `span` with byte offsets and 1-based line/column when source text is available. Type checking emits one record per error. Successful `check --json` prints `[]` to stdout.

### Example commands

```bash
# Validate
cargo run -- check examples/fizzbuzz.neb
cargo run -- check examples/fizzbuzz.neb --json

# Export structure
cargo run -- parse examples/import_demo.neb --json --load
cargo run -- ir examples/hello.neb --json

# Run with observation
cargo run -- run examples/agent_counter.neb --telemetry trace.jsonl
cargo run -- run examples/agent_counter.neb --probes probes/host.json

# Discover MCP tools before authoring probe calls
cargo run -- probes list --probes probes/mcp_stdio.json --mcp --json

# Transpile
cargo run -- compile examples/import_demo.neb --target python --out dist/
python dist/examples/import_demo.py
```

## Agent embedding

For agent runtimes that call Nebula in-process instead of shelling out to the CLI, use the `nebula-host` crate:

```rust
use nebula_host::{Host, HostConfig};

let host = Host::new();
let check = host.check_source(r#"mission main { let x: Int = 1; }"#);
assert!(check.ok);

let run = host.run_source(r#"mission main { print("Hello from Nebula"); }"#);
assert!(run.ok);
assert_eq!(run.printed, vec!["Hello from Nebula"]);
```

`HostConfig` supports probe manifests, telemetry paths, and a custom source label for diagnostics. `CheckResult` and `RunResult` return `Vec<DiagnosticJson>` with the same shape as CLI `--json` output.

## JSON schemas

Structured runtime events are documented as JSON Schema files under [`schemas/`](schemas/):

| Schema | Used by |
|--------|---------|
| [`telemetry-event.schema.json`](schemas/telemetry-event.schema.json) | `telemetry` block JSONL traces (`step`, `detail`) |
| [`probe-jsonl-event.schema.json`](schemas/probe-jsonl-event.schema.json) | `jsonl` probe handler output (`ts`, `probe`, `args`) |
| [`nebula-value.schema.json`](schemas/nebula-value.schema.json) | Probe argument values in JSONL and command-probe protocols |

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

  while i le 20 do
    print(int_to_str(math.double(i)));
    set i = i plus 1;
  end
}
```

### Sector namespacing

Symbols inside a `sector` are stored as `sector.name`:

- From `mission`, use **qualified** names: `math.double(10)`
- Inside a sector function, same-sector symbols may be **unqualified**: `double(n)`
- Builtins and mission-level probes stay unqualified: `print(...)`, `call log(...)`

### Control-flow blocks

`if`, `while`, and `telemetry` use `end`-delimited blocks. Brace blocks for control flow are rejected (`NEB-S005`):

```nebula
if count eq 0 then
  print("zero");
else
  print("nonzero");
end
```

Braces (`{ ... }`) are used for `sector`, `mission`, `fn`, and `struct` bodies only.

Comparisons use `lt`, `gt`, `le`, `ge`, `eq`, and `ne` only. Synonyms such as `less than` / `greater than` are rejected (`NEB-S004`).

### Types

`Int`, `Float`, `Bool`, `Str`, `Void`, `List<T>`, `Map<K, V>`, `Option<T>`, and function types `fn(T1, T2) -> R`. All bindings, parameters, and returns require explicit annotations.

Bare `None` is polymorphic: it type-checks against any `Option<T>`. Use `Some(x)` when you need a concrete `Option<T>`.

Empty `[]` and `{}` pick up types from annotations, parameters, or return types when available (`let xs: List<Str> = []`). Without context they default to `List<Int>` and `Map<Str, Int>`.

String concatenation uses keyword `plus`: `"Hello" plus " world"` (both operands must be `Str`).

Field access works on expressions, not just variables: `geo.origin().x`, `p.coords.y`, `(get_point()).x`.

### Builtins

Implemented in the runtime (documented in [`std/core.neb`](std/core.neb)):

| Function | Signature | Notes |
|----------|-----------|-------|
| `print` | `fn(value: Str) -> Void` | Writes to stdout |
| `len` | `fn(value: List<T> or Map<K,V> or Str) -> Int` | Element count, or string length in **code points** |
| `push` | `fn(list: List<T>, value: T) -> Void` | Mutates a **list variable** in place; first arg must be an identifier |
| `at` | `fn(list: List<T>, index: Int) -> T` | 0-based element access; out-of-range fails `NEB-R005` |
| `get` | `fn(map: Map<K,V>, key: K) -> V` | Map lookup; missing key fails `NEB-R006` |
| `has` | `fn(map: Map<K,V>, key: K) -> Bool` | Map key presence test |
| `str_to_int` | `fn(s: Str) -> Int` | |
| `int_to_str` | `fn(n: Int) -> Str` | |
| `str_to_float` | `fn(s: Str) -> Float` | |
| `float_to_str` | `fn(f: Float) -> Str` | |
| `int_to_float` | `fn(n: Int) -> Float` | Explicit widening (no implicit coercion) |
| `float_to_int` | `fn(f: Float) -> Int` | Truncates toward zero |

### Numeric semantics

Arithmetic and ordering require **both operands to share a numeric type** — both `Int` or both `Float`. There is no implicit coercion; convert with `int_to_float` / `float_to_int`. Integer `div`/`mod` truncate toward zero (the remainder's sign follows the dividend). `eq`/`ne` compare deeply, including lists, maps, options, and structs. `len` on a string counts Unicode code points. The interpreter and the Python backend produce identical results for all of these (enforced by the parity test suite).

### `return` and `emit`

Both exit the current function with a value. `return` is the conventional form; `emit` is available as an agent-friendly alias with identical semantics.

### Probes and telemetry

Probes declare capabilities the host is expected to provide. `call` invokes them through the probe host configured with `--probes <manifest.json>`:

| Handler kind | Description |
|--------------|-------------|
| `jsonl` | Built-in structured logging (`log` probe writes JSONL to stderr or a file) |
| `command` | External process with Nebula's stdin/stdout JSON protocol |
| `mcp` | Model Context Protocol tool via shared stdio or HTTP server connection |

**Command probes** use a stdin/stdout JSON protocol:

```json
{"probe":"notify","args":{"channel":"ops","message":"ready"}}
{"status":"ok"}
```

**MCP probes** map declared probes to MCP `tools/call` on servers defined in the manifest:

```json
{
  "mcp_servers": {
    "local": {
      "transport": "stdio",
      "command": ["python3", "scripts/mcp_mock_stdio.py"]
    },
    "remote": {
      "transport": "http",
      "url": "http://127.0.0.1:8765/mcp"
    }
  },
  "probes": {
    "log": { "kind": "jsonl" },
    "notify": {
      "kind": "mcp",
      "server": "local",
      "tool": "notify"
    }
  }
}
```

- One live MCP connection is reused per `mcp_servers` entry (not per `call`).
- `tool` defaults to the probe's short name if omitted.
- MCP transport failures report `NEB-P004`; tool execution errors report `NEB-P003`.

Example manifests: [`probes/host.json`](probes/host.json), [`probes/mcp_stdio.json`](probes/mcp_stdio.json). Mock MCP servers for tests: [`scripts/mcp_mock_stdio.py`](scripts/mcp_mock_stdio.py), [`scripts/mcp_mock_http.py`](scripts/mcp_mock_http.py).

```nebula
mission main {
  probe log(level: Str, message: Str) -> Void;

  telemetry
    call log(level: "info", message: "starting");
  end
}
```

Run with MCP probes:

```bash
cargo run -- run examples/agent_counter.neb --probes probes/mcp_stdio.json
```

With `--telemetry`, each statement inside a `telemetry` block appends a JSONL event matching [`schemas/telemetry-event.schema.json`](schemas/telemetry-event.schema.json).

### Imports

```nebula
import "../std/math.neb";

mission main {
  print(int_to_str(math.triple(7)));
}
```

Import paths are relative to the importing file. Library modules may contain sectors and nested imports but must not define a `mission`. Symbols are merged with sector namespacing (`math.triple`, not a flat global `triple`).

### Error codes

| Prefix | Category |
|--------|----------|
| `NEB-S` | Syntax / parse |
| `NEB-S004` | Deprecated comparison keyword (`less than`, `greater than`) |
| `NEB-S005` | Deprecated brace block for control flow |
| `NEB-T` | Type |
| `NEB-R` | Runtime |
| `NEB-P` | Probe |
| `NEB-P004` | MCP transport / protocol failure |
| `NEB-L` | Module load / import |

Type checking reports multiple errors at once with source spans (`NEB-T002`, `NEB-T009`, etc.) rather than stopping at the first failure.

## Examples

| File | Demonstrates |
|------|--------------|
| `examples/hello.neb` | Minimal program |
| `examples/fizzbuzz.neb` | Sectors, conditionals, loops |
| `examples/end_demo.neb` | `end`-delimited control-flow blocks |
| `examples/push_demo.neb` | Lists and `push` / `len` builtins |
| `examples/import_demo.neb` | Importing `std/math.neb` |
| `examples/agent_counter.neb` | Probes, telemetry, mutable state |

## Project structure

Rust workspace crates, each handling one stage of the pipeline:

| Crate | Role |
|-------|------|
| `nebula-syntax` | Lexer (logos) and hand-written recursive-descent parser |
| `nebula-ast` | Abstract syntax tree types (JSON-serializable) |
| `nebula-load` | Import resolution and program merging |
| `nebula-types` | Type checker |
| `nebula-ir` | Intermediate representation lowering |
| `nebula-runtime` | Tree-walking interpreter and probe host |
| `nebula-mcp` | MCP client (stdio + HTTP) for probe transport |
| `nebula-diagnostics` | Structured `DiagnosticJson` extraction for agent feedback |
| `nebula-host` | In-process embedding API (`check_source`, `run_source`, `list_probes`) |
| `nebula-fmt` | Canonical formatter |
| `nebula-cli` | `nebula` command-line tool |
| `nebula-python` | IR → Python transpiler and `nebula_runtime` shim bundler |
| `nebula-test-support` | Integration tests, golden files, and shared pipeline helpers (not published) |

The language specification lives in [`nebula-spec/SPEC.md`](nebula-spec/SPEC.md). The GBNF grammar for constrained generation is at [`grammar/nebula.gbnf`](grammar/nebula.gbnf).

[`std/math.neb`](std/math.neb) is an importable library module. [`std/core.neb`](std/core.neb) documents runtime builtins (builtins are not loaded from source — they are implemented in `nebula-runtime`). Probe host configuration examples live in [`probes/host.json`](probes/host.json) and [`probes/mcp_stdio.json`](probes/mcp_stdio.json); see [`scripts/probe_ok.py`](scripts/probe_ok.py) for a minimal command handler and [`scripts/mcp_mock_stdio.py`](scripts/mcp_mock_stdio.py) for a mock MCP server.

## Python transpilation

`nebula compile --target python --out <dir>` emits a runnable package:

- One `.py` module per `.neb` file in the import graph (e.g. `examples/import_demo.py`, `std/math.py`)
- A copied `nebula_runtime/` shim providing builtins, probes, telemetry, truthiness, and `NEB-R004` divide-by-zero checks
- Sector namespaces as Python classes with `@staticmethod` functions

The entry module includes `if __name__ == "__main__"` calling `run_main(main)`.

## Roadmap (not yet implemented)

- Loadable stdlib beyond importable `.neb` modules
- JSON Schema for CLI diagnostic objects

## Development

```bash
# Run all tests (unit, integration, CLI e2e, golden diagnostics/fmt)
cargo test

# Refresh golden files after intentional output changes
NEBULA_UPDATE_GOLDEN=1 cargo test -p nebula-test-support

# Format Rust code
cargo fmt

# Lint
cargo clippy
```

## License

MIT — see [LICENSE](LICENSE).
