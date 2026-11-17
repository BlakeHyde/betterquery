# bp-json

Ordered JSON parsing and serialization for BetterQuery. The `bp-parser` crate
re-exports the same `Value` type, so language values and JSON documents can be
used interchangeably without conversion or numeric precision loss.

```rust
use bp_json::{parse, to_string};

let source = r#"{"z":1.2300E+999999999999999999999,"a":[true,"𝄞"]}"#;
let value = parse(source)?;
let output = to_string(&value)?;
assert_eq!(output, source);
assert_eq!(parse(&output)?, value);
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Representation and round-trip guarantees

`Value` represents null, booleans, strings, exact number text, arrays, and ordered
object members. Objects use `Vec<(String, Value)>`, retaining input order and
allowing new members to be appended in insertion order.

- Numbers are never converted to machine integers or floating point. Their exact
  spelling, including negative zero, trailing fractional zeros, and exponent
  spelling, is retained without numeric range restrictions.
- Strings are decoded to valid Rust Unicode strings. Unpaired surrogate escapes,
  invalid escapes, and raw control characters are rejected. Unicode is not
  normalized; canonically equivalent but differently spelled keys stay distinct.
- Duplicate decoded object keys are rejected at every nesting level.
- Parsing accepts exactly one JSON value and only JSON's four whitespace
  characters. Comments, trailing commas, non-finite numbers, and extra values
  are errors.
- Serialization is compact and preserves object order. Whitespace and equivalent
  string escape spellings are **not** preserved; round trips are semantic rather
  than byte-identical, except that number spellings are preserved exactly.
- `Value` equality is structural and order-sensitive, and compares numbers by
  exact spelling. It is not RFC 6902 numeric equality.

The public enum permits constructing values directly. `to_string` validates
number text and object-key uniqueness, returning a `SerializeError` with a path
to the invalid value. Successful serialization always produces JSON that parses
back to the identical IR. Invalid Unicode cannot be represented by Rust `String`.

Parsing and serialization both allow at most 128 value levels (root at level
one). Exceeding this defensive stack limit returns an error. Parse errors report
UTF-8 byte offsets and one-based line/Unicode-character columns.

Run `cargo test -p bp-json` from `bp/` for JSON-specific tests, or
`cargo test --workspace` for the complete suite.
