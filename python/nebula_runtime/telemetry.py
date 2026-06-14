import json
from contextlib import contextmanager
from typing import Any, Dict, Iterator, Optional

STR_PREVIEW_MAX = 256

_TELEMETRY_PATH: Optional[str] = None
_TELEMETRY_ENABLED = False


def configure_telemetry(path: Optional[str]) -> None:
    global _TELEMETRY_PATH
    _TELEMETRY_PATH = path


def get_telemetry_path() -> Optional[str]:
    return _TELEMETRY_PATH


def telemetry_enabled() -> bool:
    return _TELEMETRY_ENABLED


def set_telemetry_enabled(enabled: bool) -> None:
    global _TELEMETRY_ENABLED
    _TELEMETRY_ENABLED = enabled


def value_to_json(value: Any) -> Any:
    if value is None:
        return None
    if isinstance(value, (bool, int, float, str)):
        return value
    if isinstance(value, list):
        return [value_to_json(item) for item in value]
    if isinstance(value, dict):
        if set(value.keys()) == {"Some"}:
            return {"Some": value_to_json(value["Some"])}
        if set(value.keys()) == {"struct", "fields"}:
            return {
                "struct": value["struct"],
                "fields": {k: value_to_json(v) for k, v in value["fields"].items()},
            }
        return {k: value_to_json(v) for k, v in value.items()}
    return str(value)


def telemetry_binding_value(value: Any) -> Any:
    return value_to_json(value)


def telemetry_probe_args(args: Dict[str, Any], redact: bool = False) -> Dict[str, Any]:
    if redact:
        return {key: "<redacted>" for key in args}
    return {key: value_to_json(val) for key, val in args.items()}


def telemetry_probe_result(probe_name: str, value: Any) -> Any:
    short = probe_name.rsplit(".", 1)[-1]
    if short == "secret_get":
        return {"_summary": "Str", "redacted": True}
    return _summarize_value(value)


def _summarize_value(value: Any) -> Any:
    if value is None:
        return None
    if isinstance(value, bool):
        return value
    if isinstance(value, int) and not isinstance(value, bool):
        return value
    if isinstance(value, float):
        return value
    if isinstance(value, str):
        if len(value) <= STR_PREVIEW_MAX:
            return value
        return {
            "_summary": "Str",
            "len": len(value),
            "preview": value[:STR_PREVIEW_MAX],
        }
    if isinstance(value, list):
        return {"_summary": "List", "len": len(value)}
    if isinstance(value, dict):
        if set(value.keys()) == {"Some"}:
            return {
                "_summary": "Some",
                "inner": _summarize_value(value["Some"]),
            }
        if set(value.keys()) == {"struct", "fields"}:
            return {
                "_summary": "Struct",
                "name": value["struct"],
                "fields": len(value["fields"]),
            }
        return {"_summary": "Map", "len": len(value)}
    return str(value)


@contextmanager
def telemetry_scope(path: Optional[str]) -> Iterator[None]:
    yield


def log_telemetry(
    path: Optional[str],
    enabled: bool,
    step: str,
    detail: str,
    *,
    value: Any = None,
    args: Optional[Dict[str, Any]] = None,
    result: Any = None,
) -> None:
    if not enabled or path is None:
        return
    event: Dict[str, Any] = {"step": step, "detail": detail}
    if value is not None:
        event["value"] = value
    if args is not None:
        event["args"] = args
    if result is not None:
        event["result"] = result
    with open(path, "a", encoding="utf-8") as file:
        file.write(json.dumps(event) + "\n")


def log_telemetry_probe(
    path: Optional[str],
    enabled: bool,
    probe_name: str,
    args: Dict[str, Any],
    result: Any,
) -> None:
    short = probe_name.rsplit(".", 1)[-1]
    log_telemetry(
        path,
        enabled,
        "probe",
        probe_name,
        args=telemetry_probe_args(args, redact=short == "secret_get"),
        result=telemetry_probe_result(probe_name, result),
    )