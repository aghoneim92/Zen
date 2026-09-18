# Zen

Zen is a statically typed language and planned cross-platform application platform,
with an emphasis on predictable semantics and reliable generated code. This
repository currently implements an early compiler frontend; it does not generate
or execute programs.

With stable Rust installed:

```sh
cargo build --workspace
cargo run -p zen-cli -- check tests/fixtures/valid/interfaces.zen
# Or: target/debug/zen check path/to/program.zen
cargo test --workspace
cargo clippy --workspace --all-targets --all-features
```

`zen check` exits with status 0 for valid source and 1 for source or I/O errors.
`--help` and `--version` are available. Diagnostics carry stable codes, source
ranges, and structured type information in the compiler API.

The compiler crates in `compiler/crates` separate diagnostics, syntax, semantics,
formatting, and the CLI. The core frontend uses the Rust standard library; the LSP uses tower-lsp-server and Tokio. Valid and
invalid programs live in `tests/fixtures`; invalid fixtures declare their expected
diagnostic codes, and selected diagnostics have `.stderr` snapshots.

Language inputs: [specification](docs/spec.md) and
[type system and grammar](docs/type-system-and-grammar.md). The latter takes
precedence. See [implementation notes](docs/implementation-notes.md) for draft
ambiguities, source-root rules, and current frontend limits.

Zed editor support and `zen lsp`: see [local installation and capabilities](editors/zed/README.md).
A small cross-file editor example lives in `examples/editor/src/main.zen`.

Canonical formatting: `zen fmt <path>`, `zen fmt --check .`, and `zen fmt -`.
See [formatting](docs/formatting.md) for style, CI, and editor behavior.
