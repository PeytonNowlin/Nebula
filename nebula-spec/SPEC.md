# Nebula Language Specification v0.1

Nebula is a general-purpose programming language designed for AI agent authors.
Every construct favors machine parseability over human brevity.

## 1. Lexical Structure

- Whitespace (space, tab, newline) separates tokens and is not significant.
- Line comments begin with `--` and run to end of line.
- Identifiers: `[A-Za-z_][A-Za-z0-9_]*`
- Int literals: `[0-9]+`
- Float literals: `[0-9]+\.[0-9]+`
- String literals: `"..."` with escapes `\"`, `\\`, `\n`, `\t`
- Bool literals: `true`, `false`

## 2. Keywords

```
sector mission fn struct let mut set if then else while do end emit
probe call telemetry import return
plus minus times div mod
eq ne lt gt le ge
and or not
true false
Some None
```

## 3. Types

| Type | Description |
|------|-------------|
| `Int` | 64-bit signed integer |
| `Float` | 64-bit float |
| `Bool` | Boolean |
| `Str` | UTF-8 string |
| `Void` | Unit type |
| `List<T>` | Homogeneous list |
| `Map<K, V>` | Key-value map |
| `Option<T>` | Optional value |
| `fn(T1, T2) -> R` | Function type |

All bindings, parameters, and returns require explicit type annotations.

## 4. Grammar (EBNF)

```ebnf
program        = { top_level } ;
top_level      = sector_decl | mission_decl | import_decl ;

import_decl    = "import" string_lit [ ";" ] ;
sector_decl    = "sector" ident "{" { sector_item } "}" ;
sector_item    = fn_decl | struct_decl | probe_decl ;
mission_decl   = "mission" ident "{" { mission_item } "}" ;
mission_item   = stmt | probe_decl ;

fn_decl        = "fn" ident "(" [ param_list ] ")" "->" type "{" { stmt } "}" ;
struct_decl    = "struct" ident "{" { field_decl } "}" ;
field_decl     = ident ":" type ";" ;
probe_decl     = "probe" ident "(" [ param_list ] ")" "->" type ";" ;
param_list     = param { "," param } ;
param          = ident ":" type ;

stmt           = let_stmt | set_stmt | if_stmt | while_stmt | emit_stmt
               | return_stmt | expr_stmt | telemetry_stmt | call_stmt ;
let_stmt       = "let" [ "mut" ] ident ":" type "=" expr ";" ;
set_stmt       = "set" ident "=" expr ";" ;
if_stmt        = "if" expr "then" end_block [ "else" end_block ] "end" ;
while_stmt     = "while" expr "do" end_block "end" ;
emit_stmt      = "emit" expr ";" ;
return_stmt    = "return" expr ";" ;
expr_stmt      = expr ";" ;
call_stmt      = "call" ident "(" [ arg_list ] ")" ";" ;
telemetry_stmt = "telemetry" end_block "end" ;
end_block      = { stmt } ;
arg_list       = arg { "," arg } ;
arg            = ident ":" expr ;

expr           = or_expr ;
or_expr        = and_expr { "or" and_expr } ;
and_expr       = cmp_expr { "and" cmp_expr } ;
cmp_expr       = add_expr { ( "eq" | "ne" | "lt" | "gt" | "le" | "ge" ) add_expr } ;
add_expr       = mul_expr { ( "plus" | "minus" ) mul_expr } ;
mul_expr       = unary_expr { ( "times" | "div" | "mod" ) unary_expr } ;
unary_expr     = "not" unary_expr | postfix_expr ;
postfix_expr   = primary_expr { postfix_suffix } ;
postfix_suffix = "." ident [ call_or_struct_suffix ] ;
call_or_struct_suffix = "(" [ expr_list ] ")" | "{" [ field_init { "," field_init } ] "}" ;
primary_expr   = int_lit | float_lit | string_lit | bool_lit | "None" | "Some" "(" expr ")"
               | ident [ call_or_struct_suffix ]
               | "(" expr ")" | list_lit | map_lit ;
expr_list      = expr { "," expr } ;
list_lit       = "[" [ expr_list ] "]" ;
map_lit        = "{" [ map_entry { "," map_entry } ] "}" ;
map_entry      = expr ":" expr ;
field_init     = ident ":" expr ;
```

## 5. Imports

```nebula
import "../std/math.neb";
```

- Import paths are string literals resolved relative to the importing file's directory.
- Imported files are **library modules**: they may contain `sector` declarations and nested
  `import` statements, but must not define a `mission`.
- All functions, structs, and probes from imported sectors are merged into the program's
  global symbol table.
- Duplicate symbol names across modules are a load error (`NEB-L003`).
- Circular imports are a load error (`NEB-L002`).
- Import statements are resolved before type checking and removed from the merged program.

## 6. Sector Namespacing

Symbols defined inside a `sector` are qualified as `sector.symbol`:

```nebula
sector math {
  fn double(n: Int) -> Int { return n times 2; }
}

mission main {
  print(int_to_str(math.double(10)));
}
```

- Functions, structs, and sector probes are stored as `sector.name`
- From `mission`, sector symbols must be **qualified** (`math.double`)
- Inside a sector function, same-sector symbols may be used **unqualified** (`double`)
- Builtins (`print`, `len`, `push`, etc.) and mission-level probes remain unqualified
- Types may be written as `geo.Point` or unqualified `Point` inside the `geo` sector

## 7. Semantics

- Bindings are immutable unless declared with `mut`.
- `set` requires the target binding to be `mut`.
- `mission main` is the program entry point.
- `probe` declares an external capability; `call` invokes it at runtime through the probe host.
- The probe host dispatches declared probes to handlers configured in a JSON manifest:
  - **`jsonl`** — structured JSONL logging (built-in `log` probe)
  - **`command`** — external process with Nebula stdin/stdout JSON protocol
  - **`mcp`** — Model Context Protocol `tools/call` via stdio subprocess or Streamable HTTP
- MCP manifests define shared servers under `mcp_servers` and map probes with `"kind": "mcp"`, `"server": "<id>"`, and optional `"tool"`. One connection is reused per server entry. Transport failures use `NEB-P004`. Agents can introspect manifest bindings with `nebula probes list --probes <path>`; add `--mcp` to query each server's live `tools/list` catalog.
- `telemetry` blocks append structured JSONL traces for each statement executed within. Each line matches [`schemas/telemetry-event.schema.json`](../schemas/telemetry-event.schema.json) (`step`: `let` | `set` | `probe`, `detail`: binding or probe name).
- `jsonl` probe handlers (including built-in `log`) append lines matching [`schemas/probe-jsonl-event.schema.json`](../schemas/probe-jsonl-event.schema.json). Probe argument values use the encoding in [`schemas/nebula-value.schema.json`](../schemas/nebula-value.schema.json).
- `emit` and `return` both exit the current function with a value.
- Field access uses postfix `.` on any expression: `p.x`, `geo.origin().x`, `(get_point()).x`, and chained access `p.coords.x`. A suffix `.ident` followed by `(` or `{` forms a qualified call or struct literal when the object is a name or field-access chain (e.g. `math.double(n)`, `geo.Point{ x: 0, y: 0 }`).
- Empty collection literals need a type when no context is available. `[]` defaults to `List<Int>` and `{}` defaults to `Map<Str, Int>`. When a surrounding annotation, parameter type, return type, or struct field type expects `List<T>` or `Map<K, V>`, an empty literal uses those type parameters (e.g. `let xs: List<Str> = []`, `return []` in a function returning `List<Str>`).
- Integer `div` and `mod` with a zero divisor fail at runtime with `NEB-R004` (division by zero).

### 7.1 Numeric semantics

- Arithmetic (`plus`, `minus`, `times`, `div`, `mod`) and ordering (`lt`, `gt`, `le`, `ge`) require **both operands to have the same numeric type** — either both `Int` or both `Float`. There is no implicit Int↔Float coercion; convert explicitly with `int_to_float` / `float_to_int`. Synonyms such as `less than` / `greater than` are rejected (`NEB-S004`); use `lt` / `gt`.
- `if`, `while`, and `telemetry` bodies use `end`-delimited blocks only; brace blocks are rejected (`NEB-S005`).
- `plus` is additionally defined on `Str` (concatenation) when both operands are `Str`.
- Integer `div` **truncates toward zero** and integer `mod` returns a remainder whose **sign follows the dividend** (C/Rust semantics, not Python floor division). `(0 minus 7) div 2` is `-3`; `(0 minus 7) mod 2` is `-1`.
- `Int` is a 64-bit signed integer with **checked** arithmetic: any operation whose result falls outside `[-2^63, 2^63-1]` (including `i64::MIN div -1`) raises `NEB-R007` rather than wrapping or widening. Both backends trap identically — the interpreter never wraps and the Python backend never silently promotes to a bignum.
- Float `div` is true division; float `mod` follows the dividend's sign (`fmod`). Float division/modulo by `0.0` also raises `NEB-R004`.

### 7.2 Equality and length

- `eq` / `ne` perform **deep structural comparison** for every type, including `List`, `Map`, `Option`, and struct values. Two composites are equal when their elements/fields are pairwise equal (map and struct comparison is order-independent).
- `len` on a `Str` counts **Unicode scalar values (code points)**, not bytes: `len("café")` is `4`.

Both backends (interpreter and Python transpiler) implement these semantics identically; the `nebula-python` parity test suite enforces this.

## 8. Error Codes

| Prefix | Category |
|--------|----------|
| `NEB-S` | Syntax / parse |
| `NEB-T` | Type |
| `NEB-R` | Runtime |
| `NEB-R004` | Division by zero (`div` / `mod` with zero divisor) |
| `NEB-R005` | List index out of bounds (`at` with negative or too-large index) |
| `NEB-R006` | Map key not found (`get` on an absent key) |
| `NEB-P` | Probe |
| `NEB-P004` | MCP transport / protocol failure |
| `NEB-L` | Module load / import |

## 9. Probe host manifest (MCP)

Probe manifests may include an `mcp_servers` map and probe bindings with `"kind": "mcp"`:

```json
{
  "mcp_servers": {
    "local": {
      "transport": "stdio",
      "command": ["python3", "scripts/mcp_mock_stdio.py"]
    },
    "remote": {
      "transport": "http",
      "url": "http://127.0.0.1:8765/mcp",
      "headers": { "Authorization": "Bearer token" }
    }
  },
  "probes": {
    "notify": { "kind": "mcp", "server": "local", "tool": "notify" }
  }
}
```

- **stdio** — spawn MCP server as subprocess; JSON-RPC over newline-delimited stdin/stdout
- **http** — Streamable HTTP POST to `url` with optional `headers`
- Probe `call` arguments are passed as MCP tool `arguments`
- Unknown server references or invalid transport config fail at manifest load time

## 10. Python transpilation

Nebula can be lowered to IR and transpiled to Python (`nebula compile --target python --out <dir>`).

- Output mirrors the `.neb` import graph as a multi-module package.
- Semantics are implemented by the `nebula_runtime` shim (builtins, probes, telemetry, truthiness, runtime errors).
- Sector functions become `@staticmethod` methods on sector classes; qualified calls use `sector.fn(...)`.

## 11. Builtins

Provided by the runtime standard library:

- `print(value: Str) -> Void`
- `len(value: List<T> | Map<K, V> | Str) -> Int` (string length is in code points)
- `push(list: List<T>, value: T) -> Void` (first argument must be a list variable)
- `at(list: List<T>, index: Int) -> T` (0-based; out-of-range or negative index fails with `NEB-R005`)
- `get(map: Map<K, V>, key: K) -> V` (missing key fails with `NEB-R006`)
- `has(map: Map<K, V>, key: K) -> Bool`
- `str_to_int(s: Str) -> Int`
- `int_to_str(n: Int) -> Str`
- `str_to_float(s: Str) -> Float`
- `float_to_str(f: Float) -> Str`
- `int_to_float(n: Int) -> Float`
- `float_to_int(f: Float) -> Int` (truncates toward zero)