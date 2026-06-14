# Authoring Nebula (for AI agents)

You are generating **Nebula** — a statically typed language whose toolchain
speaks JSON so you can author, validate, and run without reading human output.
Author in Nebula; it compiles to Python for deployment.

## The loop

Drive everything through one harness — [`scripts/nebula_agent.py`](scripts/nebula_agent.py):

```bash
python scripts/nebula_agent.py loop path/to/program.neb
```

It emits one JSON envelope per iteration:

- **Not ready** → `{"stage":"check","ready":false,"diagnostics":[{code,message,span}, ...]}`
  Fix the source at each `span` (byte `start`/`end`, 1-based `line`/`column`) and run again.
- **Ready** → `{"stage":"run","ready":true,"ok":<bool>,"diagnostics":[],"record":{...}}`
  The `record` has `printed`, `return_value`, `exit`, `probe_events`.

Or call the pieces directly: `check <file>` (→ `[]` or diagnostics) and
`run <file>` (→ run record). Set `$NEBULA_BIN` to the built CLI, or it falls back
to `./target/release/nebula` / `nebula` on PATH.

## Generating valid source

- Constrained-generation grammar: [`grammar/nebula.gbnf`](grammar/nebula.gbnf).
- Full spec: [`nebula-spec/SPEC.md`](nebula-spec/SPEC.md). Builtin surface:
  [`nebula-builtins/builtins.toml`](nebula-builtins/builtins.toml).

Rules that most often trip up generation:

- **Operators are keywords**, never symbols: `plus minus times div mod`,
  `eq ne lt gt le ge`, `and or not`. (`less than` is rejected — `NEB-S004`.)
- **`if`/`while`/`telemetry` use `end`-delimited blocks**, not braces (`NEB-S005`):
  `if c then ... else ... end`, `while c do ... end`.
- **Every binding, parameter, and return is type-annotated.** No implicit Int↔Float
  coercion; convert with `int_to_float` / `float_to_int`.
- One `mission main { ... }` entry point; libraries are `sector` blocks.
- Call external capabilities with `call name(arg: value);`, or capture the result:
  `let x: Int = call fetch_status(url: "...");`.

## Shipping

```bash
python scripts/nebula_agent.py ship program.neb --out dist/   # check, then compile
# or directly:
nebula compile program.neb --target python --out dist/ --json
```

`ship` validates and, if clean, compiles — returning `{"stage":"compile",
"ready":true,"record":{target,out_dir,entry_module,modules_emitted}}` (or the
`check` envelope if there are diagnostics to fix first). The output is a
self-contained Python package: run `python dist/program.py`, or import it and
call sector functions from Python (native return types). Full path:
[`docs/author-in-nebula-ship-as-python.md`](docs/author-in-nebula-ship-as-python.md).
