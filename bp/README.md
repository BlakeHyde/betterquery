# BetterQuery parser

Rust workspace containing:

- `bp-parser` (`parser/`): a parser for `../design/grammar.ebnf`.
- `bp-json` (`json/`): ordered JSON parsing and serialization with exact number
  preservation. See [`json/README.md`](json/README.md) for round-trip guarantees.

Both crates use the same `Value` intermediate representation.

```rust
use bp_parser::{parse, FormKind};

let program = parse(r#"
    (query "$..[?(@.sharedKey == true)]"
        (patch remove (path "/sharedKey")))
"#)?;
assert!(matches!(program[0].kind, FormKind::Query { .. }));
# Ok::<(), bp_parser::Error>(())
```

## API and representation

`bp_parser::parse(&str)` returns an ordered `Program` or an `Error` with a
UTF-8 byte offset and one-based line/character column. Forms carry byte spans
into the original source. Operation-specific AST variants make required patch
fields explicit; field order in the input is immaterial.

- Object members retain source order. Duplicate **decoded** keys are errors,
  including in embedded JSON; keys remain case-sensitive.
- Numbers are stored as their exact JSON lexical spelling, without conversion,
  rounding, or numeric range restrictions.
- Strings are decoded to Rust Unicode strings. Invalid escapes, raw control
  characters, and unpaired surrogate escapes are errors. No Unicode
  normalization is performed.
- Embedded JSON produces the same `Value` representation as constructors and
  is accepted in every value position. It requires exactly one JSON value
  followed by `END`; JSON literal names remain strictly lowercase.
- JSON Pointer syntax is checked after string decoding. JSONPath syntax and
  function typing are validated using `serde_json_path` with its standard
  functions enabled. Query strings are retained in decoded source form.
- Nesting is limited to 128 forms/values, returning a normal parse error.

This crate does not execute transformations. Target existence, array-index
constraints, move constraints, query selection, ordering, stable locations,
atomicity, and rollback are execution concerns, not parser checks.

## Development

From `bp/`:

```sh
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```
