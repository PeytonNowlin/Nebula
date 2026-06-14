#!/usr/bin/env python3
"""Command probe for the deploy readiness runbook (examples/runbook.neb).

Reads a Nebula probe request from stdin:
  {"probe": "fetch_status", "args": {"attempt": 1}}

Returns an HTTP-like status code as the probe's value: 503 (unavailable) until
the service becomes ready, then 200. The ready-at attempt is configurable via
NEBULA_RUNBOOK_READY_AT (default 3) so tests can drive both the success and the
retries-exhausted paths.

When NEBULA_RUNBOOK_HEALTH_LOG is set, appends a JSONL record per attempt:
  {"attempt": 1, "status": 503}

Writes a probe response to stdout:
  {"status": "ok", "value": 503}
"""

from __future__ import annotations

import json
import os
import sys


def main() -> int:
    request = json.load(sys.stdin)
    args = request.get("args") or {}
    attempt = int(args.get("attempt", 0))

    ready_at = int(os.environ.get("NEBULA_RUNBOOK_READY_AT", "3"))
    code = 200 if attempt >= ready_at else 503

    log_path = os.environ.get("NEBULA_RUNBOOK_HEALTH_LOG")
    if log_path:
        with open(log_path, "a", encoding="utf-8") as handle:
            handle.write(json.dumps({"attempt": attempt, "status": code}) + "\n")

    json.dump({"status": "ok", "value": code}, sys.stdout)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
