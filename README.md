# BetterQuery

An s-expression query and transformation language for ordered JSON documents,
combining JSON Patch, JSON Merge Patch, and JSONPath with atomic execution and
an explorable transformation timeline.

## Run the explorer

```sh
cargo run --manifest-path bp/Cargo.toml -p bp-web
```

Open <http://127.0.0.1:3000>.

See [workspace documentation](bp/README.md) for APIs, examples, and development
commands, and [design](design/00_intent) for the language intent.

## License

GNU General Public License version 3 only (`GPL-3.0-only`). See [LICENSE](LICENSE).
