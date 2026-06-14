import os
import sys
from typing import Any, Callable, Optional

from nebula_runtime.probes import RegistryProbeHost
from nebula_runtime.telemetry import (
    configure_telemetry,
    get_telemetry_path as telemetry_path,
    set_telemetry_enabled as set_telemetry,
    telemetry_enabled as is_telemetry_enabled,
)


PROBE_HOST = RegistryProbeHost()


def configure_runtime(
    probe_manifest: Optional[str] = None,
    telemetry_path: Optional[str] = None,
) -> None:
    if probe_manifest:
        PROBE_HOST.load_manifest(probe_manifest)
    configure_telemetry(telemetry_path or os.environ.get("NEBULA_TELEMETRY"))


def get_telemetry_path() -> Optional[str]:
    return telemetry_path()


def telemetry_enabled() -> bool:
    return is_telemetry_enabled()


def set_telemetry_enabled(enabled: bool) -> None:
    set_telemetry(enabled)


def run_main(
    main: Callable[[], Any],
    probe_manifest: Optional[str] = None,
    telemetry_path: Optional[str] = None,
) -> Any:
    configure_runtime(probe_manifest=probe_manifest, telemetry_path=telemetry_path)
    try:
        return main()
    except Exception as err:
        print(str(err), file=sys.stderr)
        raise SystemExit(1) from err
    finally:
        PROBE_HOST.close()