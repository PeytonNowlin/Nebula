import json
import subprocess
import sys
import time
from pathlib import Path
from typing import Any, Dict, Optional

from nebula_runtime.mcp_client import McpConnectionManager, NebulaMcpError


class NebulaProbeError(Exception):
    pass


class RegistryProbeHost:
    def __init__(self) -> None:
        self.handlers: Dict[str, Dict[str, Any]] = {
            "log": {"kind": "jsonl", "path": None},
        }
        self._mcp_servers: Dict[str, Dict[str, Any]] = {}
        self._mcp_manager: Optional[McpConnectionManager] = None

    def load_manifest(self, path: str) -> None:
        data = json.loads(Path(path).read_text(encoding="utf-8"))
        self._mcp_servers = data.get("mcp_servers", {})
        self._mcp_manager = (
            McpConnectionManager(self._mcp_servers) if self._mcp_servers else None
        )
        for name, binding in data.get("probes", {}).items():
            if binding.get("kind") == "mcp":
                server = binding.get("server")
                if not server:
                    raise NebulaProbeError(
                        f"probe `{name}` uses kind mcp but has no server"
                    )
                if self._mcp_manager is None or server not in self._mcp_servers:
                    raise NebulaProbeError(
                        f"probe `{name}` references unknown MCP server `{server}`"
                    )
            self.handlers[name] = binding

    def handler_for(self, name: str) -> Optional[Dict[str, Any]]:
        if name in self.handlers:
            return self.handlers[name]
        short = name.rsplit(".", 1)[-1]
        return self.handlers.get(short)

    @staticmethod
    def _resolve_tool_name(probe_name: str, tool: Optional[str]) -> str:
        if tool:
            return tool
        return probe_name.rsplit(".", 1)[-1]

    def call(self, name: str, args: Dict[str, Any]) -> Any:
        handler = self.handler_for(name)
        if handler is None:
            raise NebulaProbeError(
                f"NEB-P002 [probe_error] probe `{name}` is not implemented by the host"
            )
        kind = handler.get("kind")
        if kind == "jsonl":
            return self._invoke_jsonl(name, args, handler.get("path"))
        if kind == "command":
            return self._invoke_command(name, args, handler.get("command", []))
        if kind == "mcp":
            if self._mcp_manager is None:
                raise NebulaProbeError(
                    f"probe `{name}` is configured as MCP but no MCP servers are loaded"
                )
            tool = self._resolve_tool_name(name, handler.get("tool"))
            try:
                self._mcp_manager.call_tool(handler["server"], tool, args)
            except NebulaMcpError as err:
                raise NebulaProbeError(str(err)) from err
            return None
        raise NebulaProbeError(f"unknown probe handler kind for `{name}`")

    def close(self) -> None:
        if self._mcp_manager is not None:
            self._mcp_manager.close()
            self._mcp_manager = None

    def _invoke_jsonl(self, name: str, args: Dict[str, Any], path: Optional[str]) -> Any:
        event = {
            "ts": int(time.time()),
            "probe": name,
            "args": args,
        }
        line = json.dumps(event)
        if path:
            with open(path, "a", encoding="utf-8") as file:
                file.write(line + "\n")
        else:
            print(line, file=sys.stderr)
        return None

    def _invoke_command(self, name: str, args: Dict[str, Any], command: list) -> Any:
        if not command:
            raise NebulaProbeError(
                f"NEB-P003 [probe_error] probe `{name}` failed: command probe requires a non-empty command"
            )
        request = {"probe": name, "args": args}
        proc = subprocess.run(
            command,
            input=json.dumps(request),
            capture_output=True,
            text=True,
            check=False,
        )
        if proc.returncode != 0:
            raise NebulaProbeError(
                f"NEB-P003 [probe_error] probe `{name}` failed: probe command exited with status {proc.returncode}"
            )
        response = json.loads(proc.stdout)
        if response.get("status") != "ok":
            message = response.get("message", "probe command returned error status")
            raise NebulaProbeError(f"NEB-P003 [probe_error] probe `{name}` failed: {message}")
        return response.get("value")
