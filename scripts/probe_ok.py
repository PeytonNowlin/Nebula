#!/usr/bin/env python3
"""Minimal external probe handler for Nebula command probes.

Reads a JSON request from stdin:
  {"probe": "name", "args": {"key": "value"}}

Writes a JSON response to stdout:
  {"status": "ok"}
"""

import json
import sys


def main() -> int:
    json.load(sys.stdin)
    json.dump({"status": "ok"}, sys.stdout)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())