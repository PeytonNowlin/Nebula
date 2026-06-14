# Author in Nebula, ship as Python

**Nebula is the authoring layer; Python is the deployment target.** You write and
iterate agent logic in Nebula — a small, statically typed, machine-legible
language with structured JSON feedback — then compile to a plain Python package
and run it inside whatever Python infrastructure you already have.

> Think of it the way teams already think about TypeScript: you author in the
> typed language and ship the compiled output. Here, **Python is the bytecode.**

This guide documents one complete, verified path: from an agent editing `.neb`
source to a Python module imported by an existing service.

---

## 1. The authoring loop (for an AI agent)

An agent author iterates against two JSON-speaking commands. No human needs to
read the source.

```
generate .neb  ->  nebula check --json  ->  (fix diagnostics)  ->  nebula run --json
```

**`check --json`** — typecheck. On success prints `[]` and exits `0`; on failure
prints a diagnostic array to stderr and exits non-zero:

```bash
$ nebula check agent.neb --json
[]                                    # exit 0 — ready

$ nebula check broken.neb --json      # exit 1
[{"code":"NEB-T002",
  "span":{"file":"broken.neb","start":28,"end":32,"line":1,"column":29},
  "message":"type mismatch: expected Int, found Str"}]
```

Each diagnostic has a stable `code` (`NEB-S/T/R/P/L###`), a `message`, and a
`span` with byte offsets and 1-based line/column. The agent fixes the spans and
re-checks. Schema: [`schemas/diagnostic.schema.json`](../schemas/diagnostic.schema.json).

**`run --json`** — execute under the interpreter and emit one run record on
stdout (success or failure), so the agent can observe behavior without parsing
human output:

```bash
$ nebula run agent.neb --json
{"program":"agent.neb","exit":0,"diagnostics":[],"probe_events":[],
 "duration_ms":0,"printed":["alice"],"return_value":null,"probes_called":[]}
```

Schema: [`schemas/run-record.schema.json`](../schemas/run-record.schema.json).
The interpreter applies agent-safe resource limits by default (wall-clock,
loop-iteration, and memory caps) — see `--max-runtime-ms` / `--max-loop-iterations`
/ `--max-memory-mb` / `--no-resource-limits`.

This is the fast inner loop: the interpreter needs no build step, so an agent can
generate → check → fix → run in milliseconds.

---

## 2. Compile to a Python package

When the logic is correct, compile it for deployment:

```bash
nebula compile agent.neb --target python --out dist/
```

This emits a self-contained package:

```
dist/
  agent.py                 # one module per .neb file in the import graph
  nebula_runtime/          # vendored runtime shim (builtins, probes, telemetry, …)
    builtins.py  probes.py  runtime.py  values.py  ...
```

- Each `.neb` file becomes a `.py` module mirroring the import graph.
- Each `sector` becomes a Python class with `@staticmethod` functions.
- The `nebula_runtime/` shim is copied in, so the package is self-contained — no
  `pip install` of Nebula required on the deployment host (only Python 3).

---

## 3. Run it in your Python infrastructure

### As a script

```bash
python dist/agent.py
```

### As an imported module (the integration path)

The compiled package is ordinary Python. Existing infra imports it and calls in:

```python
import sys; sys.path.insert(0, "dist")
import agent

# Run the mission entry point
agent.main()

# …or call Nebula-authored sector functions directly and get native values back:
first = agent.parse.first_field("bob,25,designer")   # -> "bob"  (a Python str)
```

Nebula types map to native Python: `Int`→`int`, `Float`→`float`, `Str`→`str`,
`Bool`→`bool`, `List<T>`→`list`, `Map<K,V>`→`dict`, `Option<T>`→value-or-`None`.
So a service can author validated, typed helpers in Nebula and call them from
Python like any other module — the typing and checks happen at author time.

---

## 4. External capabilities in production (probes)

Anything the program touches outside itself — files, HTTP, MCP tools, secrets —
is a **probe**, bound at run time through a JSON manifest, never hardcoded:

```bash
nebula run agent.neb --probes probes/bundle.json          # interpreter
nebula compile agent.neb --target python --out dist/ --probes probes/bundle.json
```

Built-in bundle handlers: `read_file`, `write_file`, `http_get`, `json_parse`,
`env_get`, `secret_get`, plus `jsonl` logging, external `command` processes, and
`mcp` tool calls. Secrets are declared in the manifest's `secrets` map (resolved
from environment variables) and read with `secret_get` — secret values never
appear in `.neb` source. See [`probes/bundle.json`](../probes/bundle.json).

---

## 5. Prove the artifact in CI

The recommended production pipeline compiles to Python in CI and runs the
artifact, so the deployment path is exercised on every change:

```yaml
- name: Build Python deployment artifact
  run: |
    cargo run --release -- compile examples/io_agent.neb --target python --out dist/
    python dist/examples/io_agent.py
```

The interpreter and the Python backend are kept in lockstep by a parity test
suite, so behavior you verified with `run --json` matches what ships.

---

## Summary

| Stage | Tool | Output |
|-------|------|--------|
| Author / iterate | `nebula check --json`, `nebula run --json` | diagnostics, run records |
| Compile | `nebula compile --target python` | self-contained Python package |
| Deploy | `python …` or `import …` | script or importable module |
| Integrate | `import agent; agent.sector.fn(...)` | native Python values |

Author in Nebula. Ship as Python.
