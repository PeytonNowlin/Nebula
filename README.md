# Nebula

[![CI](https://github.com/PeytonNowlin/Nebula/actions/workflows/ci.yml/badge.svg)](https://github.com/PeytonNowlin/Nebula/actions/workflows/ci.yml)

**A small, statically typed language built for AI agent authors.**

Nebula trades symbolic brevity for machine legibility: operators are keywords (`plus`, `eq`, `lt`), types are always explicit, and the toolchain speaks JSON — structured diagnostics, AST export, IR export, and run records with stable `NEB-*` error codes.

**Author in Nebula, ship as Python.** Agents iterate in Nebula against `check --json` / `run --json` (driven by the [`scripts/nebula_agent.py`](scripts/nebula_agent.py) loop harness — see [**AGENTS.md**](AGENTS.md)), then `compile --target python` produces a self-contained package you run in existing Python infrastructure — Nebula is the typed authoring layer, Python is the bytecode. Full path: [**docs/author-in-nebula-ship-as-python.md**](docs/author-in-nebula-ship-as-python.md).

This repository is the reference implementation: a Rust compiler and interpreter with a Python backend, probe host, and in-process embedding API.

**Status:** v0.1 — language and toolchain are experimental but fully tested.

## Hello, Nebula

```nebula
mission main {
  print("Hello from Nebula");
}
```

```bash
cargo build --release
./target/release/nebula run examples/hello.neb
# Hello from Nebula
```

## Requirements

- [Rust](https://www.rust-lang.org/) (2021 edition; recent stable)
- [Python 3](https://www.python.org/) (transpiled output, MCP probe servers, parity tests)

## The `nebula` CLI

Build once, then invoke subcommands directly or via `cargo run --`:

| Command | Purpose |
|---------|---------|
| `check <file>` | Parse, resolve imports, typecheck |
| `run <file>` | Typecheck and execute |
| `parse <file> --json [--load]` | Export AST (optionally merged workspace) |
| `ir <file> --json` | Export lowered IR |
| `fmt <file> [--write]` | Canonical formatter |
| `compile <file> --target python --out <dir> [--json]` | Transpile to Python (`--json` emits a compile record: target, out dir, entry module, module count) |
| `probes list --probes <manifest> [--mcp] [--json]` | Inspect probe bindings and MCP tools |

### Agent-oriented flags

**Validation**

```bash
nebula check examples/fizzbuzz.neb
nebula check examples/fizzbuzz.neb --json   # success → `[]` on stdout; failure → JSON on stderr
```

**Execution**

```bash
nebula run examples/agent_counter.neb
nebula run examples/agent_counter.neb --telemetry trace.jsonl
nebula run examples/agent_counter.neb --probes probes/bundle.json
nebula run examples/runbook.neb --probes probes/runbook.json --json
```

**Resource limits** (interpreter defaults: 30s wall clock, 1M while-loop iterations, 64 MiB approximate memory)

```bash
nebula run examples/hello.neb --max-runtime-ms 5000
nebula run examples/hello.neb --max-loop-iterations 100000
nebula run examples/hello.neb --max-memory-mb 128
nebula run examples/hello.neb --no-resource-limits
```

**Introspection and transpilation**

```bash
nebula parse examples/import_demo.neb --json --load
nebula ir examples/hello.neb --json
nebula probes list --probes probes/mcp_stdio.json --mcp --json
nebula compile examples/import_demo.neb --target python --out dist/
python dist/examples/import_demo.py
```

### JSON output

`check --json` emits a diagnostic array on **stderr** when validation fails:

```json
[
  {
    "code": "NEB-T002",
    "span": { "file": "example.neb", "start": 42, "end": 58, "line": 3, "column": 15 },
    "message": "type mismatch: expected Int, found Str"
  }
]
```

`run --json` always emits one **run record** on **stdout** (success or failure). Schema: [`schemas/run-record.schema.json`](schemas/run-record.schema.json).

```json
{
  "program": "examples/runbook.neb",
  "exit": 0,
  "diagnostics": [],
  "telemetry_path": "trace.jsonl",
  "probe_events": [
    { "ts": 1718380800, "probe": "log", "args": { "level": "info", "message": "ready" } }
  ],
  "duration_ms": 42,
  "printed": ["deploy ok"],
  "return_value": null,
  "probes_called": []
}
```

Other schemas live under [`schemas/`](schemas/): diagnostics, telemetry events, probe JSONL, probe manifests, and Nebula value encoding.

## Language overview

Full specification: [`nebula-spec/SPEC.md`](nebula-spec/SPEC.md). Constrained-generation grammar: [`grammar/nebula.gbnf`](grammar/nebula.gbnf).

A program is a set of `sector` libraries, `import` statements, and exactly one `mission` entry point (conventionally `main`).

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

### Design choices

| Topic | Behavior |
|-------|----------|
| **Operators** | Keywords, not symbols: `plus`, `minus`, `times`, `div`, `mod`, `eq`, `lt`, … |
| **Control flow** | `if` / `while` / `telemetry` use `end`-delimited blocks; brace blocks for control flow are rejected (`NEB-S005`) |
| **Comparisons** | Only `lt`, `gt`, `le`, `ge`, `eq`, `ne`; synonyms like `less than` are rejected (`NEB-S004`) |
| **Types** | All bindings, parameters, and returns are annotated; no implicit coercion |
| **Namespacing** | Sector symbols are qualified (`math.double`); same-sector calls may be unqualified inside the sector |
| **Numeric semantics** | Matching `Int`/`Int` or `Float`/`Float` operands; checked 64-bit integer arithmetic (`NEB-R007` on overflow); `div`/`mod` truncate toward zero |
| **Collections** | `List<T>`, `Map<K,V>`, `Option<T>`; empty `[]` / `{}` infer from context or default to `List<Int>` / `Map<Str,Int>` |
| **Strings** | UTF-8; `len` and string builtins operate on Unicode code points; both backends agree (parity-tested) |

### Builtins

Builtins are implemented in the runtime — not loaded from source. The canonical surface is [`nebula-builtins/builtins.toml`](nebula-builtins/builtins.toml) (signatures stay in sync across the typechecker, interpreter, and Python shim). Human-readable notes: [`std/core.neb`](std/core.neb).

| Category | Functions |
|----------|-----------|
| I/O | `print` |
| Collections | `len`, `push`, `at`, `get`, `has`, `insert` |
| Conversions | `str_to_int`, `int_to_str`, `str_to_float`, `float_to_str`, `int_to_float`, `float_to_int` |
| Strings | `substr`, `contains`, `index_of`, `starts_with`, `ends_with`, `to_upper`, `to_lower`, `trim`, `replace`, `split`, `join` |
| Integers | `abs`, `min`, `max` |

`push` and `insert` mutate a **variable** in place; their first argument must be an identifier.

### Imports

```nebula
import "../std/math.neb";

mission main {
  print(int_to_str(math.triple(7)));
}
```

Import paths are string literals, resolved relative to the importing file. Library modules may define sectors and nested imports but not a `mission`. Duplicate symbols across modules fail at load time (`NEB-L003`).

Importable standard module: [`std/math.neb`](std/math.neb).

### Probes and telemetry

Probes declare host-provided capabilities; `call` dispatches them through a JSON manifest (`--probes`):

```nebula
mission main {
  probe log(level: Str, message: Str) -> Void;

  telemetry
    call log(level: "info", message: "starting");
  end
}
```

| Handler kind | Role |
|--------------|------|
| `jsonl` | Structured logging (`log` probe) |
| `command` | External process with stdin/stdout JSON protocol |
| `mcp` | Model Context Protocol tool call (stdio or HTTP) |
| `read_file`, `write_file`, `http_get`, `json_parse`, `env_get`, `secret_get` | Native bundle handlers |

Example manifests: [`probes/host.json`](probes/host.json), [`probes/bundle.json`](probes/bundle.json), [`probes/mcp_stdio.json`](probes/mcp_stdio.json), [`probes/runbook.json`](probes/runbook.json).

Secrets are declared in the manifest `secrets` map (resolved from environment variables), substituted into handler config via `${secret:name}`, and read at runtime with `secret_get` — never embed secret values in `.neb` source.

With `--telemetry`, statements inside `telemetry` blocks append JSONL events ([`schemas/telemetry-event.schema.json`](schemas/telemetry-event.schema.json)).

### Error codes

| Prefix | Category |
|--------|----------|
| `NEB-S` | Syntax / lex |
| `NEB-T` | Type checking |
| `NEB-L` | Module load / imports |
| `NEB-R` | Runtime |
| `NEB-P` | Probes / MCP |

Type checking reports multiple errors with spans in one pass.

## Embedding (`nebula-host`)

For agent loops that should not shell out to the CLI:

```rust
use nebula_host::{Host, HostConfig, ResourceLimits};

let host = Host::with_config(HostConfig {
    probe_manifest: Some("probes/bundle.json".into()),
    telemetry_path: Some("trace.jsonl".into()),
    resource_limits: ResourceLimits::agent_defaults(),
    ..HostConfig::default()
});

let check = host.check_source(r#"mission main { let x: Int = 1; }"#);
assert!(check.ok);

let run = host.run_file("examples/hello.neb");
assert!(run.ok);
assert_eq!(run.printed, vec!["Hello from Nebula"]);
// run.record is a RunRecord (same shape as `nebula run --json`)
```

Pipeline stages are also available directly via `Host::try_parse_file`, `try_compile_file`, `try_emit_python`, and `list_probes`.

## Compiler pipeline

```
.neb source
  → nebula-syntax   (lex + parse)
  → nebula-load     (imports, workspace merge)
  → nebula-types    (typecheck; builtins from nebula-builtins)
  → nebula-ir       (lower)
  → nebula-runtime  (interpret)  |  nebula-python (transpile)
```

| Crate | Role |
|-------|------|
| `nebula-builtins` | Canonical builtin manifest (`builtins.toml`) |
| `nebula-syntax` | Lexer and parser |
| `nebula-ast` | AST types, `NebError`, JSON diagnostic types |
| `nebula-load` | Import graph and symbol merge |
| `nebula-types` | Type checker |
| `nebula-ir` | Intermediate representation |
| `nebula-runtime` | Interpreter, probe host, resource limits |
| `nebula-mcp` | MCP client transport |
| `nebula-host` | Unified pipeline and embedding API |
| `nebula-diagnostics` | `miette::Report` → JSON diagnostic extraction |
| `nebula-fmt` | Formatter |
| `nebula-python` | Python transpiler and runtime shim |
| `nebula-cli` | `nebula` binary |
| `nebula-test-support` | Integration tests and golden files (internal) |

## Examples

| File | Demonstrates |
|------|--------------|
| [`examples/hello.neb`](examples/hello.neb) | Minimal program |
| [`examples/fizzbuzz.neb`](examples/fizzbuzz.neb) | Sectors, conditionals, loops |
| [`examples/end_demo.neb`](examples/end_demo.neb) | `end`-delimited control flow |
| [`examples/push_demo.neb`](examples/push_demo.neb) | Lists, `push`, `len` |
| [`examples/import_demo.neb`](examples/import_demo.neb) | Importing `std/math.neb` |
| [`examples/agent_counter.neb`](examples/agent_counter.neb) | Probes, telemetry, mutable state |
| [`examples/io_agent.neb`](examples/io_agent.neb) | Bundle probes (`read_file`, `http_get`, …) |
| [`examples/runbook.neb`](examples/runbook.neb) | Retry loop, command + MCP probes |
| [`examples/agent_lib.neb`](examples/agent_lib.neb) + [`agent_lib_harness.py`](examples/agent_lib_harness.py) | Compile a Nebula library, import and call it from Python |

## Python backend

`nebula compile --target python --out <dir>` emits:

- One `.py` module per `.neb` file in the import graph
- A copied `nebula_runtime/` shim (builtins, probes, telemetry, checked arithmetic)
- Sector namespaces as Python classes with `@staticmethod` functions

The Rust interpreter and Python backend are kept in parity by an integration test suite.

## Development

```bash
cargo test                  # full workspace: unit, integration, CLI e2e, parity
cargo fmt && cargo clippy
NEBULA_UPDATE_GOLDEN=1 cargo test -p nebula-test-support   # refresh golden files
```

## Roadmap

- Package manager / module registry beyond relative imports
- Additional compile targets beyond Python
- Standard library modules beyond `std/math.neb`

## License

MIT — see [LICENSE](LICENSE).
