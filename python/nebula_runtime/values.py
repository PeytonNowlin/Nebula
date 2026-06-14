from dataclasses import dataclass
from typing import Any, Dict


@dataclass
class StructValue:
    name: str
    fields: Dict[str, Any]


def nebula_field(obj: Any, field: str) -> Any:
    if isinstance(obj, StructValue):
        if field not in obj.fields:
            raise RuntimeError(f"unknown field `{field}`")
        return obj.fields[field]
    raise RuntimeError("field access on non-struct")


def nebula_key(value: Any) -> str:
    if isinstance(value, str):
        return value
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, int):
        return str(value)
    if isinstance(value, float):
        return str(value)
    if value is None:
        return "None"
    if isinstance(value, StructValue):
        return value.name
    raise RuntimeError("unsupported map key type")