# verse-rs

`verse-rs` is a Rust compiler and bytecode VM for a Verse-aligned language
subset.

It includes a lexer, parser, semantic checker, bytecode compiler, VM runtime,
CLI, REPL, examples, diagnostics, project loading, and digest generation. The
goal is to be useful for language experiments, tooling, and embedders that need
a practical Verse-like runtime.

This is not a complete Epic Games Verse or UEFN runtime implementation.

## Quick Start

```powershell
cargo run -- examples/maps.verse
cargo run -- check examples/classes.verse
cargo run -- ast examples/fizzbuzz.verse
cargo run
```

The default build enables the `tokio-host` feature. That means the CLI and REPL
use the Tokio-backed host by default.

## A Small Program

```verse
var Scores:[string]int = map{
    "alice" => 10,
    "bob" => 20,
}

Updated:int = if (set Scores["alice"] += 5). 1 else. 0

Selected:[]int = for (Name -> Score : Scores, Score > 10):
    Score

print("alice=" + str(if (Score := Scores["alice"]). Score else. 0))
print("entries=" + str(Scores.Length))
print("selected=" + str(Selected.Length))
```

Output:

```text
alice=15
entries=2
selected=2
```

This example shows the parts of the implementation that are already useful in
practice: typed maps, failable lookups, speculative mutation in failure
contexts, `for` expressions, mutable state, and builtin string conversion.

## CLI

When installed as a binary, the command shape is:

```text
verse-rs <file>
verse-rs run <file>
verse-rs check <file>
verse-rs ast <file>
verse-rs
```

The no-argument form starts the REPL. The REPL keeps bindings between inputs:

```text
verse> x:int = 41
41
verse> x + 1
42
```

Diagnostics are source-highlighted for parse, check, and runtime failures.

## Library Usage

```rust
use verse_rs::{check_source, run_source, run_source_with_tokio_host};

let ty = check_source("40 + 2")?;
let value = run_source("40 + 2")?;
let hosted = run_source_with_tokio_host("40 + 2")?;
```

Use `run_source(...)` or `run_project_file(...)` for the deterministic
in-process mock host. Use `run_source_with_tokio_host(...)` or
`run_project_file_with_tokio_host(...)` when you want Tokio-backed timers and
futures.

The Tokio APIs are available behind the `tokio-host` feature, which is enabled
by default:

```toml
verse-rs = { version = "0.1", default-features = true }
```

For a smaller deterministic build:

```toml
verse-rs = { version = "0.1", default-features = false }
```

## Examples

The `examples/` directory is the best place to see the currently supported
surface in executable form.

| File | Covers |
| --- | --- |
| `arrays.verse` | arrays, indexing, slices, updates, array helpers |
| `classes.verse` | classes, inheritance, interfaces, constructors, fields, methods |
| `enums.verse` | enums, qualified enum values, `case` |
| `factorial.verse` | recursion and simple functions |
| `fizzbuzz.verse` | loops, branching, arithmetic, printing |
| `functions.verse` | function definitions, effects, named/default arguments, builtins |
| `loops.verse` | ranges, `for`, `loop`, `break`, expression loops |
| `maps.verse` | maps, failable lookup, map updates, map iteration |
| `options.verse` | options, `option{}`, unwraps, optional member chains |
| `returns.verse` | early return and `defer` |
| `structs.verse` | structs, archetypes, fields, equality |
| `tuples.verse` | tuple literals, tuple annotations, indexing |

Each file should run directly:

```powershell
cargo run -- examples/fizzbuzz.verse
```

## Supported Surface

The implementation intentionally tracks documented Verse syntax where possible.
The current executable subset includes:

- Lexing and parsing for Verse comments, identifiers, literals, strings,
  interpolation, path-qualified names, blocks, and expression forms.
- Static checking for primitives, arrays, maps, tuples, options, enums,
  structs, classes, interfaces, modules, access specifiers, type aliases,
  effects, and failure contexts.
- Functions with recursion, overloads, extension methods, named/default
  arguments, function types, inline type parameters, and selected `where`
  constraints.
- Runtime execution through bytecode for expressions, bindings, mutation,
  functions, methods, collections, options, `if`, `case`, `for`, `loop`,
  `return`, `break`, `defer`, and rollback inside failure contexts.
- Builtin helpers for printing, conversion, diagnostics, arrays, maps, math,
  random values, colors, results, sessions, simulation time, and selected
  generated interfaces.
- Async and structured concurrency support for `spawn`, `Sleep`, `task`,
  `event`, `sync`, `race`, `rush`, and `branch` in the VM scheduler.
- Local module loading from sibling `.verse` files and module folders,
  project-file loading, digest generation, and package-aware checking/running.
- Modeled UEFN-style platform types such as `agent`, `player`, `entity`,
  `component`, `tag`, `session`, `weak_map`, persistence-related annotations,
  and common native interfaces.

For exact behavior, prefer the executable examples and tests over this summary.

## Project Files

File-mode CLI runs can load local module definitions from sibling
`ModuleName.verse` files and `ModuleName/` folders. The project loader also
supports package-aware checking/running through `SourceProject`,
`check_project_file(...)`, `run_project_file(...)`, and
`run_project_file_with_tokio_host(...)`.

Use digest generation when you need a stable representation of a checked source
or project:

```rust
use verse_rs::{generate_digest, generate_project_digest};
```

## Current Limits

- This is not a drop-in replacement for Epic's compiler, UEFN editor
  integration, asset system, persistence backend, or live simulation runtime.
- Platform APIs are modeled far enough for checking and local VM execution, but
  they do not connect to real UEFN services.
- Host/native package-wide capability propagation is still incomplete.
- A few checker-accepted edge forms are intentionally not promoted as examples
  until their lowering/runtime behavior is fully exercised.

## Development Checks

Before publishing or pushing a release candidate, run:

```powershell
cargo fmt -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo test --no-default-features
cargo package
```

## License

MIT
