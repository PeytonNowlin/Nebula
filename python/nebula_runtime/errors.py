class NebulaRuntimeError(Exception):
    """Runtime error from transpiled Nebula code."""


class NebulaDivideByZero(NebulaRuntimeError):
    def __str__(self) -> str:
        return "NEB-R004 [runtime_error] division by zero"