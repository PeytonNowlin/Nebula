class NebulaRuntimeError(Exception):
    """Runtime error from transpiled Nebula code."""


class NebulaDivideByZero(NebulaRuntimeError):
    def __str__(self) -> str:
        return "NEB-R004 [runtime_error] division by zero"


class NebulaIndexError(NebulaRuntimeError):
    def __init__(self, index: int, length: int) -> None:
        super().__init__(index, length)
        self.index = index
        self.length = length

    def __str__(self) -> str:
        return (
            f"NEB-R005 [runtime_error] list index {self.index} "
            f"out of bounds (len {self.length})"
        )


class NebulaKeyError(NebulaRuntimeError):
    def __init__(self, key: str) -> None:
        super().__init__(key)
        self.key = key

    def __str__(self) -> str:
        return f"NEB-R006 [runtime_error] key `{self.key}` not found in map"


class NebulaIntegerOverflow(NebulaRuntimeError):
    def __init__(self, op: str) -> None:
        super().__init__(op)
        self.op = op

    def __str__(self) -> str:
        return f"NEB-R007 [runtime_error] integer overflow in `{self.op}`"