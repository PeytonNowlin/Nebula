from nebula_runtime.errors import NebulaRuntimeError
from nebula_runtime.values import nebula_key

__all__ = [
    "nebula_print",
    "nebula_len",
    "nebula_push",
    "nebula_at",
    "nebula_get",
    "nebula_has",
    "nebula_insert",
    "nebula_str_to_int",
    "nebula_int_to_str",
    "nebula_str_to_float",
    "nebula_float_to_str",
    "nebula_int_to_float",
    "nebula_float_to_int",
    "nebula_add",
    "nebula_sub",
    "nebula_mul",
    "nebula_div",
    "nebula_mod",
]

_I64_MIN = -(2**63)
_I64_MAX = 2**63 - 1


def _check_i64(value: int, op: str) -> int:
    # Nebula's Int is 64-bit; trap overflow as NEB-R007 to match the Rust
    # interpreter instead of silently producing a Python bignum.
    if value < _I64_MIN or value > _I64_MAX:
        from nebula_runtime.errors import NebulaIntegerOverflow

        raise NebulaIntegerOverflow(op)
    return value


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


def nebula_at(xs: list, index: int):
    if not isinstance(xs, list):
        raise NebulaRuntimeError("at expects a List as first argument")
    if not _is_int(index):
        raise NebulaRuntimeError("at index must be an Int")
    # No Python-style negative indexing: match the interpreter's bounds check.
    if index < 0 or index >= len(xs):
        from nebula_runtime.errors import NebulaIndexError

        raise NebulaIndexError(index, len(xs))
    return xs[index]


def nebula_get(m: dict, key):
    if not isinstance(m, dict):
        raise NebulaRuntimeError("get expects a Map as first argument")
    k = nebula_key(key)
    if k not in m:
        from nebula_runtime.errors import NebulaKeyError

        raise NebulaKeyError(k)
    return m[k]


def nebula_has(m: dict, key) -> bool:
    if not isinstance(m, dict):
        raise NebulaRuntimeError("has expects a Map as first argument")
    return nebula_key(key) in m


def nebula_insert(m: dict, key, value) -> None:
    if not isinstance(m, dict):
        raise NebulaRuntimeError("insert expects a Map as first argument")
    # Mutates in place (the dict is shared by reference), matching the
    # interpreter and the `push` builtin.
    m[nebula_key(key)] = value


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


def nebula_add(left, right):
    if _is_int(left) and _is_int(right):
        return _check_i64(left + right, "plus")
    return left + right  # float addition or string concatenation


def nebula_sub(left, right):
    if _is_int(left) and _is_int(right):
        return _check_i64(left - right, "minus")
    return left - right


def nebula_mul(left, right):
    if _is_int(left) and _is_int(right):
        return _check_i64(left * right, "times")
    return left * right


def nebula_div(left, right):
    if right == 0:
        from nebula_runtime.errors import NebulaDivideByZero

        raise NebulaDivideByZero()
    if _is_int(left) and _is_int(right):
        # Truncate toward zero to match the Rust interpreter (NEB integer `div`
        # is C-style, not Python floor division).
        q = abs(left) // abs(right)
        q = -q if (left < 0) != (right < 0) else q
        return _check_i64(q, "div")  # traps i64::MIN / -1
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