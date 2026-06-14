# nebula-runtime

Runtime shim for [Nebula](https://github.com/PeytonNowlin/Nebula)'s Python
backend. It provides the builtins, probe host, telemetry, checked arithmetic, and
value helpers that programs compiled with `nebula compile --target python` rely
on at run time.

## Two ways to ship

**Vendored (default).** `nebula compile --target python --out dist/` copies this
package into `dist/nebula_runtime/`, so the output is self-contained — no install
required on the deployment host beyond Python 3.

**Shared dependency.** Teams running many compiled Nebula modules in one Python
environment can install the runtime once instead of vendoring a copy per build:

```bash
pip install nebula-runtime          # from PyPI (when published)
pip install ./python                # from a checkout of this repo
```

With the package installed, compiled modules resolve `from nebula_runtime import …`
from the environment, and the vendored `dist/nebula_runtime/` directory can be
removed. See [`docs/author-in-nebula-ship-as-python.md`](../docs/author-in-nebula-ship-as-python.md).
