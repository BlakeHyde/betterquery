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

## Nix

```sh
nix develop
nix build
nix run
```

`nix develop` provides Cargo, rustfmt, Clippy, and rust-analyzer.
`nix build` builds the workspace, runs its tests, and installs `bp-web`.
`nix run` starts the explorer at <http://127.0.0.1:3000>.
The first argument to `nix run` is the flake, so pass explorer arguments
after an explicit `.`:

```sh
nix run . -- --listen 127.0.0.1:8080
```

Inside the development shell, run the workspace commands in
[bp/README.md](bp/README.md) with `--manifest-path bp/Cargo.toml`.

## License

GNU General Public License version 3 only (`GPL-3.0-only`). See [LICENSE](LICENSE).
