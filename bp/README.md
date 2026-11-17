# BetterQuery

BetterQuery applies s-expression programs to ordered JSON documents using
JSON Patch (RFC 6902), JSON Merge Patch (RFC 7396), and JSONPath (RFC 9535).
The Cargo workspace follows [`design/20_code_structure`](../design/20_code_structure).

## Run the explorer

From this directory:

```sh
cargo run -p bp-web
```

Open <http://127.0.0.1:3000>. Enter a JSON document and a BetterQuery program,
then select **Run program**. The server renders the query tree, ordered matches,
transformation source, intermediate documents, and final result. The form and
expandable timeline work without JavaScript. Examples include key removal,
merge patches, stable array selection, and rollback.

To choose a different bind address:

```sh
cargo run -p bp-web -- --listen 127.0.0.1:8080
```

The explorer keeps no server-side document state. Requests are limited to 1 MiB.

## Workspace

| Directory | Crate | Responsibility |
| --- | --- | --- |
| `common/` | `bp-common` | Ordered values, program IR, JSON Pointers, document node identities, exact number comparisons, execution errors |
| `parser/` | `bp-parser` | BetterQuery syntax, source spans, pointer and query validation |
| `document_parser/` | `bp-document-parser` | Strict ordered JSON parsing and serialization |
| `json_patch/` | `bp-json-patch` | All six JSON Patch operations, atomically, relative to a document or subtree |
| `json_merge_patch/` | `bp-json-merge-patch` | JSON Merge Patch, preserving surviving member order and node identities |
| `json_path/` | `bp-json-path` | JSONPath selection, standard functions, exact filter comparisons, ordered de-duplicated results |
| `timeline/` | `bp-timeline` | Atomic program execution, nested queries, frozen targets, events, rollback diagnostics |
| `web/` | `bp-web` | Server-rendered document explorer |
| `json/` | `bp-json` | Compatibility exports for the original JSON crate |

## Execute a program

```rust
use bp_timeline::evaluate;

let timeline = evaluate(
    r#"(query "$[1,3]" (patch remove (path "")))"#,
    r#"["a","b","c","d","e"]"#,
)?;
assert!(timeline.succeeded());
assert_eq!(
    bp_document_parser::to_string(&timeline.result.unwrap())?,
    r#"["a","c","e"]"#,
);
# Ok::<(), Box<dyn std::error::Error>>(())
```

`evaluate(program, document)` returns an input error for malformed source.
Execution errors are retained in `Timeline::error`, with a restored result and
an explicit rollback event. `run(&mut Document, &Program)` commits the document
only if the entire program succeeds. Every event records its parent query,
source span, subtree path, and whole-document before/after snapshots. Query
events also record the selected values and their original paths.

For lower-level operations, use `bp_json_patch::apply`,
`bp_json_merge_patch::apply`, or their `apply_at` variants, and
`bp_json_path::select` or `query`. `select` returns stable identities and paths;
`query` returns the selected values. Each patch and merge call is atomic.

## Execution rules

- Top-level forms run in source order against the preceding result.
- Each query snapshots its matches before running its single body form.
  Matches are de-duplicated and sorted depth-first, parents before children,
  with object keys in document order. New keys append in patch input order.
- Frozen query targets follow the original nodes through array index shifts.
  Replacing or removing an ancestor invalidates its replaced or removed
  descendants, even if the same paths or values reappear. Mutating an object
  in place preserves its identity and the identities of untouched descendants.
- An invalidated target fails the whole program. Its error reports the failing
  form and the transformation before which the target was last valid. The
  timeline retains speculative changes for inspection, followed by rollback.
- In nested queries, `$` denotes the currently selected subtree. Patch pointers
  and `from` pointers are relative to that subtree.
- Removing the document root produces no document (`None`), distinct from JSON
  null. A subsequent root `add` or `merge` can produce a new document.
- Patch `test` ignores object-key order and compares numbers mathematically.
  Query filter comparisons also use exact decimal arithmetic, including nested
  values, without expanding large exponents. Internal JSONPath nodelists retain
  duplicates for functions such as `count`; final transformation targets do not.

## JSON representation

Object members retain input order. Duplicate decoded keys are errors in both
JSON and BetterQuery objects; keys remain case-sensitive. Numbers retain their
exact JSON spelling without rounding or numeric range restrictions. Strings
are valid Rust Unicode strings: invalid escapes, raw control characters, and
unpaired surrogate escapes are errors. Unicode normalization is not performed.

`Value` structural equality compares number spelling and member order. Use
`bp_common::equivalent` for JSON mathematical equality. Serialization preserves
values, member order, and number spelling, while whitespace and string escape
spellings may change. See [`json/README.md`](json/README.md).

The current document parser accepts JSON. YAML decoding is not part of the
JSON document parser described in the code structure. Parsing and serialization
limit document nesting to 128 levels. Execution rejects results beyond that
limit and restores the original document. Queries are limited to 64 KiB,
128 nested delimiters, and 512 logical-operator characters. These bounds are
checked before recursive validation. The standard `match` and `search` functions
validate I-Regexp (RFC 9485) syntax and use the Unicode-aware Rust regex engine,
including its compilation resource limits.

## Development

```sh
cargo test --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
```

From the repository root, the Nix flake provides the same toolchain and a
tested `bp-web` package:

```sh
nix develop
nix build
nix run
```

The tests cover parser and JSON round trips, patch semantics and rollback,
RFC 7396 Appendix A, JSONPath selectors and standard functions, exact numbers,
query snapshot ordering and identities, nested execution, timeline snapshots,
and HTTP form handling and HTML escaping.
