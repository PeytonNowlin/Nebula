#!/usr/bin/env python3
"""Command probe handler that returns an integer status code.

Reads a JSON request from stdin:
  {"probe": "name", "args": {"url": "..."}}

Writes a JSON response to stdout:
  {"status": "ok", "value": 200}
"""

import json
import sys


def main() -> int:
    json.load(sys.stdin)
    json.dump({"status": "ok", "value": 200}, sys.stdout)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())