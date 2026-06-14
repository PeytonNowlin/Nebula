#!/usr/bin/env python3
"""Command probe handler for the deploy readiness runbook integration test.

Reads a Nebula probe request from stdin:
  {"probe": "check_health", "args": {"attempt": "1"}}

Appends a JSONL health record when NEBULA_RUNBOOK_HEALTH_LOG is set:
  {"attempt": 1, "status": "unhealthy"}

Writes a probe response to stdout:
  {"status": "ok"}
"""

from __future__ import annotations

import json
import os
import sys


def main() -> int:
    request = json.load(sys.stdin)
    args = request.get("args") or {}
    attempt = int(args.get("attempt", "0"))

    log_path = os.environ.get("NEBULA_RUNBOOK_HEALTH_LOG")
    if log_path:
        status = "healthy" if attempt >= 3 else "unhealthy"
        with open(log_path, "a", encoding="utf-8") as handle:
            handle.write(json.dumps({"attempt": attempt, "status": status}) + "\n")

    json.dump({"status": "ok"}, sys.stdout)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
