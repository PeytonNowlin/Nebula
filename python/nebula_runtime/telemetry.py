import json
from contextlib import contextmanager
from typing import Iterator, Optional


@contextmanager
def telemetry_scope(path: Optional[str]) -> Iterator[None]:
    yield


def log_telemetry(path: Optional[str], enabled: bool, step: str, detail: str) -> None:
    if not enabled or path is None:
        return
    event = {"step": step, "detail": detail}
    with open(path, "a", encoding="utf-8") as file:
        file.write(json.dumps(event) + "\n")