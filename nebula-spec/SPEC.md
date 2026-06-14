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
less than greater than
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
if_stmt        = "if" expr "then" block [ "else" block ] end_block_suffix ;
while_stmt     = "while" expr "do" block end_block_suffix ;
end_block_suffix = /* empty for brace blocks */ | "end" ;
emit_stmt      = "emit" expr ";" ;
return_stmt    = "return" expr ";" ;
expr_stmt      = expr ";" ;
call_stmt      = "call" ident "(" [ arg_list ] ")" ";" ;
telemetry_stmt = "telemetry" block end_block_suffix ;
block          = brace_block | end_block ;
brace_block    = "{" { stmt } "}" ;
end_block      = { stmt } ;
arg_list       = arg { "," arg } ;
arg            = ident ":" expr ;

expr           = or_expr ;
or_expr        = and_expr { "or" and_expr } ;
and_expr       = cmp_expr { "and" cmp_expr } ;
cmp_expr       = add_expr { ( "eq" | "ne" | "lt" | "gt" | "le" | "ge" | "less" "than" | "greater" "than" ) add_expr } ;
add_expr       = mul_expr { ( "plus" | "minus" ) mul_expr } ;
mul_expr       = unary_expr { ( "times" | "div" | "mod" ) unary_expr } ;
unary_expr     = "not" unary_expr | primary_expr ;
primary_expr   = int_lit | float_lit | string_lit | bool_lit | "None" | "Some" "(" expr ")"
               | ident | field_access | call_expr | "(" expr ")" | list_lit | map_lit | struct_lit ;
field_access   = ident "." ident ;
call_expr      = ident "(" [ expr_list ] ")" ;
expr_list      = expr { "," expr } ;
list_lit       = "[" [ expr_list ] "]" ;
map_lit        = "{" [ map_entry { "," map_entry } ] "}" ;
map_entry      = expr ":" expr ;
struct_lit     = ident "{" [ field_init { "," field_init } ] "}" ;
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
- `probe` declares an external capability; `call` invokes it at runtime.
- `telemetry` blocks append structured JSONL traces for each statement executed within.
- `emit` and `return` both exit the current function with a value.

## 8. Error Codes

| Prefix | Category |
|--------|----------|
| `NEB-S` | Syntax / parse |
| `NEB-T` | Type |
| `NEB-R` | Runtime |
| `NEB-P` | Probe |
| `NEB-L` | Module load / import |

## 9. Builtins

Provided by the runtime standard library:

- `print(value: Str) -> Void`
- `len(value: List<T>) -> Int`
- `push(list: List<T>, value: T) -> Void`
- `str_to_int(s: Str) -> Int`
- `int_to_str(n: Int) -> Str`