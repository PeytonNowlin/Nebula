import os
import re
from typing import Dict, Optional

SECRET_PATTERN = re.compile(r"\$\{secret:([^}]+)\}")


def resolve_secrets(
    bindings: Dict[str, dict],
    overlay: Optional[Dict[str, str]] = None,
) -> Dict[str, str]:
    store: Dict[str, str] = {}
    for name, binding in bindings.items():
        if "env" in binding:
            env_name = binding["env"]
            value = os.environ.get(env_name)
            if value is None:
                raise ValueError(
                    f"secret `{name}` references unset environment variable `{env_name}`"
                )
            store[name] = value
        elif "value" in binding:
            store[name] = binding["value"]
        else:
            raise ValueError(f"invalid secret binding for `{name}`")
    if overlay:
        store.update(overlay)
    return store


def substitute_secrets(template: str, store: Dict[str, str]) -> str:
    def replace(match: re.Match[str]) -> str:
        name = match.group(1)
        if name not in store:
            raise ValueError(f"unknown secret `{name}` in template `{template}`")
        return store[name]

    return SECRET_PATTERN.sub(replace, template)


def substitute_string_map(values: Dict[str, str], store: Dict[str, str]) -> Dict[str, str]:
    return {key: substitute_secrets(value, store) for key, value in values.items()}