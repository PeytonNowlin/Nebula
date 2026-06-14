from nebula_runtime.errors import NebulaRuntimeError

__all__ = [
    "nebula_print",
    "nebula_len",
    "nebula_push",
    "nebula_str_to_int",
    "nebula_int_to_str",
    "nebula_str_to_float",
    "nebula_float_to_str",
    "nebula_int_to_float",
    "nebula_float_to_int",
    "nebula_div",
    "nebula_mod",
]


def nebula_print(value) -> None:
    if isinstance(value, str):
        print(value)
    elif isinstance(value, bool):
        print("true" if value else "false")
    elif isinstance(value, int):
        print(str(value))
    elif isinstance(value, float):
        print(str(value))
    else:
        raise NebulaRuntimeError("print expects Str-compatible value")


def nebula_len(value) -> int:
    if isinstance(value, (str, list, dict)):
        return len(value)
    raise NebulaRuntimeError("len expects List or Str")


def nebula_push(xs: list, value) -> None:
    if not isinstance(xs, list):
        raise NebulaRuntimeError("push expects List as first argument")
    xs.append(value)


def nebula_str_to_int(value: str) -> int:
    if not isinstance(value, str):
        raise NebulaRuntimeError("str_to_int expects Str")
    try:
        return int(value)
    except ValueError as err:
        raise NebulaRuntimeError(f"invalid integer string: {err}") from err


def nebula_int_to_str(value: int) -> str:
    if not isinstance(value, int):
        raise NebulaRuntimeError("int_to_str expects Int")
    return str(value)


def nebula_str_to_float(value: str) -> float:
    if not isinstance(value, str):
        raise NebulaRuntimeError("str_to_float expects Str")
    try:
        return float(value)
    except ValueError as err:
        raise NebulaRuntimeError(f"invalid float string: {err}") from err


def nebula_float_to_str(value: float) -> str:
    if not isinstance(value, float):
        raise NebulaRuntimeError("float_to_str expects Float")
    return str(value)


def nebula_int_to_float(value: int) -> float:
    if not _is_int(value):
        raise NebulaRuntimeError("int_to_float expects Int")
    return float(value)


def nebula_float_to_int(value: float) -> int:
    if not isinstance(value, float):
        raise NebulaRuntimeError("float_to_int expects Float")
    return int(value)  # truncates toward zero


def _is_int(value) -> bool:
    # bool is a subclass of int in Python; Nebula never reaches div/mod with a
    # Bool operand (the typechecker forbids it), but guard anyway.
    return isinstance(value, int) and not isinstance(value, bool)


def nebula_div(left, right):
    if right == 0:
        from nebula_runtime.errors import NebulaDivideByZero

        raise NebulaDivideByZero()
    if _is_int(left) and _is_int(right):
        # Truncate toward zero to match the Rust interpreter (NEB integer `div`
        # is C-style, not Python floor division).
        q = abs(left) // abs(right)
        return -q if (left < 0) != (right < 0) else q
    return left / right


def nebula_mod(left, right):
    if right == 0:
        from nebula_runtime.errors import NebulaDivideByZero

        raise NebulaDivideByZero()
    if _is_int(left) and _is_int(right):
        # Remainder whose sign follows the dividend, matching Rust `%`.
        return left - nebula_div(left, right) * right
    import math

    return math.fmod(left, right)