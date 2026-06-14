import json
import os
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any, Dict, Optional

from nebula_runtime.mcp_client import McpConnectionManager, NebulaMcpError
from nebula_runtime.secrets import resolve_secrets, substitute_secrets, substitute_string_map
from nebula_runtime.telemetry import (
    get_telemetry_path,
    log_telemetry_probe,
    telemetry_enabled,
)


class NebulaProbeError(Exception):
    pass


class RegistryProbeHost:
    def __init__(self) -> None:
        self.handlers: Dict[str, Dict[str, Any]] = {
            "log": {"kind": "jsonl", "path": None},
        }
        self._mcp_servers: Dict[str, Dict[str, Any]] = {}
        self._mcp_manager: Optional[McpConnectionManager] = None
        self._secrets: Dict[str, str] = {}

    def load_manifest(
        self,
        path: str,
        secrets_overlay: Optional[Dict[str, str]] = None,
    ) -> None:
        data = json.loads(Path(path).read_text(encoding="utf-8"))
        try:
            self._secrets = resolve_secrets(data.get("secrets", {}), secrets_overlay)
        except ValueError as err:
            raise NebulaProbeError(str(err)) from err

        mcp_servers = data.get("mcp_servers", {})
        for config in mcp_servers.values():
            if "env" in config:
                config["env"] = substitute_string_map(config["env"], self._secrets)
            if "headers" in config:
                config["headers"] = substitute_string_map(config["headers"], self._secrets)

        probes = data.get("probes", {})
        for binding in probes.values():
            kind = binding.get("kind")
            if kind == "command":
                binding["command"] = [
                    substitute_secrets(arg, self._secrets) for arg in binding.get("command", [])
                ]
                if "env" in binding:
                    binding["env"] = substitute_string_map(binding["env"], self._secrets)
            elif kind == "http_get" and "headers" in binding:
                binding["headers"] = substitute_string_map(binding["headers"], self._secrets)

        self._mcp_servers = mcp_servers
        self._mcp_manager = (
            McpConnectionManager(self._mcp_servers) if self._mcp_servers else None
        )
        for name, binding in probes.items():
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
            result = self._invoke_jsonl(name, args, handler.get("path"))
        elif kind == "command":
            result = self._invoke_command(
                name,
                args,
                handler.get("command", []),
                handler.get("env", {}),
            )
        elif kind == "mcp":
            if self._mcp_manager is None:
                raise NebulaProbeError(
                    f"probe `{name}` is configured as MCP but no MCP servers are loaded"
                )
            tool = self._resolve_tool_name(name, handler.get("tool"))
            try:
                self._mcp_manager.call_tool(handler["server"], tool, args)
            except NebulaMcpError as err:
                raise NebulaProbeError(str(err)) from err
            result = None
        elif kind == "read_file":
            result = self._invoke_read_file(name, args)
        elif kind == "write_file":
            result = self._invoke_write_file(name, args)
        elif kind == "http_get":
            result = self._invoke_http_get(name, args, handler.get("headers"))
        elif kind == "json_parse":
            result = self._invoke_json_parse(name, args)
        elif kind == "env_get":
            result = self._invoke_env_get(name, args)
        elif kind == "secret_get":
            result = self._invoke_secret_get(name, args)
        else:
            raise NebulaProbeError(f"unknown probe handler kind for `{name}`")

        log_telemetry_probe(
            get_telemetry_path(),
            telemetry_enabled(),
            name,
            args,
            result,
        )
        return result

    def close(self) -> None:
        if self._mcp_manager is not None:
            self._mcp_manager.close()
            self._mcp_manager = None

    def _invoke_jsonl(self, name: str, args: Dict[str, Any], path: Optional[str]) -> Any:
        short = name.rsplit(".", 1)[-1]
        logged_args = (
            {key: "<redacted>" for key in args}
            if short == "secret_get"
            else args
        )
        event = {
            "ts": int(time.time()),
            "probe": name,
            "args": logged_args,
        }
        line = json.dumps(event)
        if path:
            with open(path, "a", encoding="utf-8") as file:
                file.write(line + "\n")
        else:
            print(line, file=sys.stderr)
        return None

    def _invoke_command(
        self,
        name: str,
        args: Dict[str, Any],
        command: list,
        env: Optional[Dict[str, str]] = None,
    ) -> Any:
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
            env={**os.environ, **(env or {})},
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

    def _required_str_arg(self, name: str, args: Dict[str, Any], key: str) -> str:
        if key not in args:
            raise NebulaProbeError(
                f"NEB-P003 [probe_error] probe `{name}` failed: missing required argument `{key}`"
            )
        value = args[key]
        if not isinstance(value, str):
            raise NebulaProbeError(
                f"NEB-P003 [probe_error] probe `{name}` failed: argument `{key}` must be Str"
            )
        return value

    def _invoke_read_file(self, name: str, args: Dict[str, Any]) -> str:
        path = self._required_str_arg(name, args, "path")
        try:
            return Path(path).read_text(encoding="utf-8")
        except OSError as err:
            raise NebulaProbeError(
                f"NEB-P003 [probe_error] probe `{name}` failed: {err}"
            ) from err

    def _invoke_write_file(self, name: str, args: Dict[str, Any]) -> None:
        path = self._required_str_arg(name, args, "path")
        content = self._required_str_arg(name, args, "content")
        try:
            Path(path).write_text(content, encoding="utf-8")
        except OSError as err:
            raise NebulaProbeError(
                f"NEB-P003 [probe_error] probe `{name}` failed: {err}"
            ) from err
        return None

    def _invoke_http_get(
        self,
        name: str,
        args: Dict[str, Any],
        headers: Optional[Dict[str, str]] = None,
    ) -> str:
        url = self._required_str_arg(name, args, "url")
        request = urllib.request.Request(url, headers=headers or {})
        try:
            with urllib.request.urlopen(request) as response:
                return response.read().decode("utf-8")
        except (OSError, urllib.error.URLError) as err:
            raise NebulaProbeError(
                f"NEB-P003 [probe_error] probe `{name}` failed: {err}"
            ) from err

    def _invoke_json_parse(self, name: str, args: Dict[str, Any]) -> Dict[str, Any]:
        text = self._required_str_arg(name, args, "text")
        try:
            value = json.loads(text)
        except json.JSONDecodeError as err:
            raise NebulaProbeError(
                f"NEB-P003 [probe_error] probe `{name}` failed: {err}"
            ) from err
        if not isinstance(value, dict):
            raise NebulaProbeError(
                f"NEB-P003 [probe_error] probe `{name}` failed: json_parse requires a JSON object at the top level"
            )
        return value

    def _invoke_env_get(self, name: str, args: Dict[str, Any]) -> Optional[str]:
        key = self._required_str_arg(name, args, "key")
        return os.environ.get(key)

    def _invoke_secret_get(self, name: str, args: Dict[str, Any]) -> Optional[str]:
        key = self._required_str_arg(name, args, "name")
        return self._secrets.get(key)