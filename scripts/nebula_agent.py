#!/usr/bin/env python3
"""nebula_agent — the authoring-loop harness for AI agents.

Nebula's CLI already speaks JSON; this wraps it into one Python entry point so an
agent (or a service, or CI) can drive the loop without parsing human output:

    generate .neb  ->  check()  ->  (fix diagnostics)  ->  run()

The "generate" and "fix" steps belong to the model. This harness owns the
deterministic glue: locating the binary, invoking `check`/`run`, and parsing the
structured results.

Library use:

    from nebula_agent import check, run, author_loop

    diags = check("agent.neb")          # [] when ready, else [{code, message, span}, ...]
    for d in diags:
        ...                             # feed code/span/message back to the model to fix
    record = run("agent.neb")           # {printed, return_value, exit, diagnostics, ...}

    step = author_loop("agent.neb")     # one envelope per iteration (see below)

CLI use:

    python nebula_agent.py check agent.neb
    python nebula_agent.py run   agent.neb --probes probes/bundle.json
    python nebula_agent.py loop  agent.neb            # check, then run if clean

Binary resolution order: $NEBULA_BIN, then ./target/release/nebula relative to
this repo, then `nebula` on PATH.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Any, Optional


def _nebula_bin() -> list[str]:
    env = os.environ.get("NEBULA_BIN")
    if env:
        return [env]
    local = Path(__file__).resolve().parent.parent / "target" / "release" / "nebula"
    if local.exists():
        return [str(local)]
    return ["nebula"]  # assume on PATH


def _harness_diagnostic(message: str) -> dict[str, Any]:
    """A synthetic diagnostic for failures the compiler did not report as JSON
    (missing file, binary not found, malformed output)."""
    return {"code": "NEB-HARNESS", "message": message, "span": None}


def check(path: str, *, probes: Optional[str] = None) -> list[dict[str, Any]]:
    """Typecheck `path`. Returns a list of diagnostics — empty means ready to run.

    `check --json` prints `[]` on success (stdout, exit 0) and a diagnostic array
    on failure (stderr, exit 1)."""
    cmd = _nebula_bin() + ["check", str(path), "--json"]
    if probes:
        cmd += ["--probes", str(probes)]
    proc = subprocess.run(cmd, capture_output=True, text=True)
    payload = (proc.stdout if proc.returncode == 0 else proc.stderr).strip()
    if not payload:
        if proc.returncode == 0:
            return []
        return [_harness_diagnostic(proc.stderr.strip() or "check failed")]
    try:
        return json.loads(payload)
    except json.JSONDecodeError:
        return [_harness_diagnostic(payload)]


def run(
    path: str,
    *,
    probes: Optional[str] = None,
    telemetry: Optional[str] = None,
) -> dict[str, Any]:
    """Execute `path` and return its run record (printed output, return value,
    exit code, diagnostics, probe events). `run --json` always emits one record
    on stdout, success or failure."""
    cmd = _nebula_bin() + ["run", str(path), "--json"]
    if probes:
        cmd += ["--probes", str(probes)]
    if telemetry:
        cmd += ["--telemetry", str(telemetry)]
    proc = subprocess.run(cmd, capture_output=True, text=True)
    try:
        return json.loads(proc.stdout)
    except json.JSONDecodeError:
        return {
            "program": str(path),
            "exit": proc.returncode,
            "diagnostics": [_harness_diagnostic(proc.stderr.strip() or "run failed")],
            "printed": [],
            "return_value": None,
        }


def author_loop(path: str, *, probes: Optional[str] = None) -> dict[str, Any]:
    """One iteration of the agent loop, as a single envelope:

    - not ready: ``{"stage": "check", "ready": False, "diagnostics": [...]}``
      — the model fixes the spans and calls again.
    - ready:     ``{"stage": "run", "ready": True, "ok": <exit==0>,
                    "diagnostics": [...], "record": {...}}``
    """
    diags = check(path, probes=probes)
    if diags:
        return {"stage": "check", "ready": False, "diagnostics": diags}
    record = run(path, probes=probes)
    return {
        "stage": "run",
        "ready": True,
        "ok": record.get("exit") == 0,
        "diagnostics": record.get("diagnostics", []),
        "record": record,
    }


def _main(argv: Optional[list[str]] = None) -> int:
    parser = argparse.ArgumentParser(description="Nebula authoring-loop harness")
    sub = parser.add_subparsers(dest="cmd", required=True)
    for name in ("check", "run", "loop"):
        p = sub.add_parser(name)
        p.add_argument("file")
        p.add_argument("--probes")
    args = parser.parse_args(argv)

    if args.cmd == "check":
        diags = check(args.file, probes=args.probes)
        print(json.dumps(diags))
        return 0 if not diags else 1
    if args.cmd == "run":
        record = run(args.file, probes=args.probes)
        print(json.dumps(record))
        return int(record.get("exit", 1) or 0)
    # loop
    step = author_loop(args.file, probes=args.probes)
    print(json.dumps(step))
    if not step["ready"]:
        return 1
    return 0 if step["ok"] else 1


if __name__ == "__main__":
    sys.exit(_main())
