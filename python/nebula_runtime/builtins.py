from nebula_runtime.errors import NebulaRuntimeError


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


def nebula_div(left: int, right: int) -> int:
    if right == 0:
        from nebula_runtime.errors import NebulaDivideByZero

        raise NebulaDivideByZero()
    return left // right


def nebula_mod(left: int, right: int) -> int:
    if right == 0:
        from nebula_runtime.errors import NebulaDivideByZero

        raise NebulaDivideByZero()
    return left % right