import os
import sys
from typing import Any, Callable, Optional

from nebula_runtime.probes import RegistryProbeHost
from nebula_runtime.telemetry import log_telemetry


PROBE_HOST = RegistryProbeHost()
_TELEMETRY_PATH: Optional[str] = None
_TELEMETRY_ENABLED = False


def configure_runtime(
    probe_manifest: Optional[str] = None,
    telemetry_path: Optional[str] = None,
) -> None:
    global _TELEMETRY_PATH
    if probe_manifest:
        PROBE_HOST.load_manifest(probe_manifest)
    _TELEMETRY_PATH = telemetry_path or os.environ.get("NEBULA_TELEMETRY")


def get_telemetry_path() -> Optional[str]:
    return _TELEMETRY_PATH


def telemetry_enabled() -> bool:
    return _TELEMETRY_ENABLED


def set_telemetry_enabled(enabled: bool) -> None:
    global _TELEMETRY_ENABLED
    _TELEMETRY_ENABLED = enabled


def run_main(main: Callable[[], Any], probe_manifest: Optional[str] = None, telemetry_path: Optional[str] = None) -> Any:
    configure_runtime(probe_manifest=probe_manifest, telemetry_path=telemetry_path)
    try:
        return main()
    except Exception as err:
        print(str(err), file=sys.stderr)
        raise SystemExit(1) from err