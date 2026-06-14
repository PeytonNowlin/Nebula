from __future__ import annotations

import json
import os
import subprocess
import sys
import threading
import urllib.error
import urllib.request
from typing import Any, Dict, Optional


class NebulaMcpError(Exception):
    pass


class _StdioMcpSession:
    def __init__(self, command: list[str], env: Optional[Dict[str, str]] = None) -> None:
        self._process = subprocess.Popen(
            command,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
            env=env,
        )
        self._next_id = 1
        self._initialized = False

    def _next_request_id(self) -> int:
        request_id = self._next_id
        self._next_id += 1
        return request_id

    def _write(self, message: Dict[str, Any]) -> None:
        assert self._process.stdin is not None
        self._process.stdin.write(json.dumps(message) + "\n")
        self._process.stdin.flush()

    def _read_response(self, expected_id: int) -> Dict[str, Any]:
        assert self._process.stdout is not None
        while True:
            line = self._process.stdout.readline()
            if not line:
                raise NebulaMcpError("NEB-P004 [probe_error] MCP transport error: unexpected EOF")
            message = json.loads(line)
            if message.get("id") == expected_id:
                return message

    def _ensure_initialized(self) -> None:
        if self._initialized:
            return
        init_id = self._next_request_id()
        self._write(
            {
                "jsonrpc": "2.0",
                "id": init_id,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": {"name": "nebula", "version": "0.1.0"},
                },
            }
        )
        response = self._read_response(init_id)
        if "error" in response:
            raise NebulaMcpError(
                f"NEB-P004 [probe_error] MCP transport error: {response['error']['message']}"
            )
        self._write({"jsonrpc": "2.0", "method": "notifications/initialized"})
        self._initialized = True

    def call_tool(self, tool: str, arguments: Dict[str, Any]) -> None:
        self._ensure_initialized()
        request_id = self._next_request_id()
        self._write(
            {
                "jsonrpc": "2.0",
                "id": request_id,
                "method": "tools/call",
                "params": {"name": tool, "arguments": arguments},
            }
        )
        response = self._read_response(request_id)
        if "error" in response:
            raise NebulaMcpError(
                f"NEB-P003 [probe_error] MCP tool `{tool}` failed: {response['error']['message']}"
            )
        result = response.get("result") or {}
        if result.get("isError"):
            text = "\n".join(
                item.get("text", "")
                for item in result.get("content", [])
                if item.get("type") == "text"
            )
            raise NebulaMcpError(
                f"NEB-P003 [probe_error] MCP tool `{tool}` failed: {text or 'isError=true'}"
            )

    def close(self) -> None:
        if self._process.poll() is None:
            self._process.kill()
            self._process.wait()


class _HttpMcpSession:
    def __init__(self, url: str, headers: Optional[Dict[str, str]] = None) -> None:
        self._url = url
        self._headers = headers or {}
        self._session_id: Optional[str] = None
        self._next_id = 1
        self._initialized = False

    def _next_request_id(self) -> int:
        request_id = self._next_id
        self._next_id += 1
        return request_id

    def _post(self, message: Dict[str, Any]) -> Dict[str, Any]:
        payload = json.dumps(message).encode("utf-8")
        headers = {
            "Content-Type": "application/json",
            "Accept": "application/json, text/event-stream",
            **self._headers,
        }
        if self._session_id:
            headers["Mcp-Session-Id"] = self._session_id
        request = urllib.request.Request(self._url, data=payload, headers=headers, method="POST")
        try:
            with urllib.request.urlopen(request) as response:
                session_id = response.headers.get("Mcp-Session-Id")
                if session_id:
                    self._session_id = session_id
                body = response.read().decode("utf-8")
        except urllib.error.HTTPError as err:
            body = err.read().decode("utf-8")
            raise NebulaMcpError(
                f"NEB-P004 [probe_error] MCP transport error: HTTP {err.code}: {body}"
            ) from err
        except urllib.error.URLError as err:
            raise NebulaMcpError(
                f"NEB-P004 [probe_error] MCP transport error: {err.reason}"
            ) from err

        trimmed = body.strip()
        if trimmed.startswith("event:") or "\ndata:" in trimmed:
            for line in trimmed.splitlines():
                if line.startswith("data:"):
                    data = line[len("data:") :].strip()
                    if data:
                        return json.loads(data)
            raise NebulaMcpError(
                "NEB-P004 [probe_error] MCP transport error: invalid SSE response"
            )
        return json.loads(trimmed)

    def _ensure_initialized(self) -> None:
        if self._initialized:
            return
        init_id = self._next_request_id()
        response = self._post(
            {
                "jsonrpc": "2.0",
                "id": init_id,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": {"name": "nebula", "version": "0.1.0"},
                },
            }
        )
        if "error" in response:
            raise NebulaMcpError(
                f"NEB-P004 [probe_error] MCP transport error: {response['error']['message']}"
            )
        self._post({"jsonrpc": "2.0", "method": "notifications/initialized"})
        self._initialized = True

    def call_tool(self, tool: str, arguments: Dict[str, Any]) -> None:
        self._ensure_initialized()
        request_id = self._next_request_id()
        response = self._post(
            {
                "jsonrpc": "2.0",
                "id": request_id,
                "method": "tools/call",
                "params": {"name": tool, "arguments": arguments},
            }
        )
        if "error" in response:
            raise NebulaMcpError(
                f"NEB-P003 [probe_error] MCP tool `{tool}` failed: {response['error']['message']}"
            )
        result = response.get("result") or {}
        if result.get("isError"):
            text = "\n".join(
                item.get("text", "")
                for item in result.get("content", [])
                if item.get("type") == "text"
            )
            raise NebulaMcpError(
                f"NEB-P003 [probe_error] MCP tool `{tool}` failed: {text or 'isError=true'}"
            )

    def close(self) -> None:
        return


class McpConnectionManager:
    def __init__(self, servers: Dict[str, Dict[str, Any]]) -> None:
        self._servers = servers
        self._sessions: Dict[str, Any] = {}
        self._lock = threading.Lock()

    def call_tool(self, server_id: str, tool: str, arguments: Dict[str, Any]) -> None:
        with self._lock:
            if server_id not in self._sessions:
                config = self._servers.get(server_id)
                if config is None:
                    raise NebulaMcpError(
                        f"MCP configuration error: unknown MCP server `{server_id}` in probe manifest"
                    )
                transport = config.get("transport")
                if transport == "stdio":
                    command = config.get("command") or []
                    if not command:
                        raise NebulaMcpError(
                            f"MCP configuration error: mcp_servers.{server_id} requires non-empty `command`"
                        )
                    env_config = config.get("env") or {}
                    env = os.environ.copy()
                    env.update({str(k): str(v) for k, v in env_config.items()})
                    self._sessions[server_id] = _StdioMcpSession(command, env=env)
                elif transport == "http":
                    url = config.get("url")
                    if not url:
                        raise NebulaMcpError(
                            f"MCP configuration error: mcp_servers.{server_id} requires `url`"
                        )
                    self._sessions[server_id] = _HttpMcpSession(
                        url, headers=config.get("headers")
                    )
                else:
                    raise NebulaMcpError(
                        f"MCP configuration error: unsupported transport `{transport}`"
                    )
            session = self._sessions[server_id]
        session.call_tool(tool, arguments)

    def close(self) -> None:
        with self._lock:
            for session in self._sessions.values():
                session.close()
            self._sessions.clear()
