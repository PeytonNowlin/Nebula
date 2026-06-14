#!/usr/bin/env python3
"""Minimal MCP stdio server for Nebula integration tests."""

from __future__ import annotations

import json
import sys
from typing import Any, Dict


def write_message(message: Dict[str, Any]) -> None:
    sys.stdout.write(json.dumps(message) + "\n")
    sys.stdout.flush()


def handle_request(message: Dict[str, Any]) -> None:
    request_id = message.get("id")
    method = message.get("method")

    if method == "initialize":
        write_message(
            {
                "jsonrpc": "2.0",
                "id": request_id,
                "result": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {"tools": {}},
                    "serverInfo": {"name": "mcp_mock_stdio", "version": "0.1.0"},
                },
            }
        )
        return

    if method == "tools/list":
        write_message(
            {
                "jsonrpc": "2.0",
                "id": request_id,
                "result": {
                    "tools": [
                        {
                            "name": "notify",
                            "description": "Send a notification to a channel",
                        }
                    ]
                },
            }
        )
        return

    if method == "tools/call":
        params = message.get("params") or {}
        tool = params.get("name")
        if tool == "notify":
            write_message(
                {
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "result": {
                        "content": [{"type": "text", "text": "ok"}],
                        "isError": False,
                    },
                }
            )
            return
        write_message(
            {
                "jsonrpc": "2.0",
                "id": request_id,
                "result": {
                    "content": [{"type": "text", "text": f"unknown tool: {tool}"}],
                    "isError": True,
                },
            }
        )
        return

    if request_id is not None:
        write_message(
            {
                "jsonrpc": "2.0",
                "id": request_id,
                "error": {"code": -32601, "message": f"Method not found: {method}"},
            }
        )


def main() -> int:
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        message = json.loads(line)
        if message.get("method") == "notifications/initialized":
            continue
        if "id" in message:
            handle_request(message)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
