# Zen

Zen is a statically typed language and planned cross-platform application
platform focused on predictable semantics and reliable generated code. The
repository currently provides a compiler frontend, typed high-level intermediate
representation (HIR), a reference interpreter with deterministic async tasks, a
canonical formatter, a language server, and Zed integration. Production code
generation and the application platform remain future work.

## Getting started

Use stable Rust with the `rustfmt` and `clippy` components; the repository's
`rust-toolchain.toml` configures these. A native C build toolchain is required by
the interpreter’s stack-growth dependency (for example, Xcode Command Line Tools
on macOS). Run commands from the repository root:

```sh
cargo build --workspace
cargo run -p zen-cli -- check tests/fixtures/valid/interfaces.zen
cargo run -p zen-cli -- check examples/editor/src/main.zen
cargo run -p zen-cli -- run tests/conformance/functions/calls.zen
cargo run -p zen-cli -- run tests/conformance/async/basic.zen
cargo run -p zen-cli -- --help
```

To install the `zen` binary on your PATH:

```sh
cargo install --path compiler/crates/zen-cli --locked
```

Ensure your Cargo binary directory (normally `$HOME/.cargo/bin`) is on `PATH`.
Alternatively, use `cargo run -p zen-cli --` or `target/debug/zen` without installing.

## Commands and tooling

| Command | Behavior |
| --- | --- |
| `zen run <file>` | Check, lower to HIR, and execute the entry file’s `main`. |
| `zen check <file>` | Parse, resolve imports, and type-check a program. |
| `zen fmt <path>...` | Format files or recursively format directories. |
| `zen fmt --check <path>...` | Check formatting without rewriting files. |
| `zen fmt -` | Read source from stdin and write formatted source to stdout. |
| `zen lsp` | Start the language server over stdio. |
| `zen --help` / `zen --version` | Show usage or version information. |

`zen check` returns 0 for valid source, 1 for source or I/O errors, and 2 for invalid
command usage. Diagnostics carry stable codes, source ranges, and structured type
information in the compiler API. Imports use the nearest ancestor named `src` as
the source root, or the input file's directory otherwise. There is no package
manifest or dependency registry yet.

`zen run` uses a parameterless, non-generic `main` returning `Unit`, `Int`, `Bool`
or `String`. Success exits 0; the returned value is neither printed nor used as an
exit code. Async `main` is driven to completion; native calls are unbound.
See the [reference interpreter guide](docs/interpreter.md) for APIs and limits.

Formatting is deterministic and shared by the CLI and language server. Syntax
errors prevent formatting that file; type errors do not. Avoid in-place formatting
of intentionally invalid or unformatted test fixtures. See the
[formatting guide](docs/formatting.md) for canonical style, exit codes, traversal
rules, and editor behavior.

Zed uses Tree-sitter for syntax and `zen lsp` for compiler-backed diagnostics,
navigation, completion, rename, semantic tokens, inlay hints, quick fixes, and
whole-document formatting. Follow the [Zed setup guide](editors/zed/README.md) to
prepare and install the development extension. The cross-file
[editor example](examples/editor/src/main.zen) is useful for checking integration.

## Repository layout

The compiler is a Rust 2024 workspace. The core frontend uses the Rust standard
library; the interpreter uses num-bigint and stacker, and the LSP uses
tower-lsp-server and Tokio.

| Path | Purpose |
| --- | --- |
| `compiler/crates/zen-diagnostics/` | Source maps, spans, and structured diagnostics. |
| `compiler/crates/zen-syntax/` | Lexer, parser, AST, recovery, and syntax metadata. |
| `compiler/crates/zen-semantics/` | Resolution, type checking, and shared IDE analysis. |
| `compiler/crates/zen-hir/` | Owned typed/resolved HIR, lowering, and deterministic dumps; see the [HIR contract](docs/hir.md). |
| `compiler/crates/zen-interpreter/` | HIR execution, deterministic task executor, values, host bridge, and execution tests. |
| `compiler/crates/zen-format/` | Canonical formatter and its regression fixtures. |
| `compiler/crates/zen-lsp/` | Language server, workspace snapshots, and editor features. |
| `compiler/crates/zen-cli/` | The `zen` command and CLI/LSP integration tests. |
| `tests/conformance/` | Backend-neutral execution source/result fixtures. |
| `tests/fixtures/` | Valid and invalid frontend programs and selected diagnostic snapshots. |
| `tooling/tree-sitter-zen/` | Syntax-only grammar and corpus/query validation. |
| `editors/zed/` | Separate extension crate, language queries, and development setup. |
| `examples/editor/` | Cross-file example program for editor testing. |
| `docs/` | Language design, implementation decisions, and feature guides. |

`zen_hir::lower_program` converts successfully checked programs into owned, typed,
resolved HIR for the reference interpreter and future backends. It preserves
source order, semantic identities,
resolved calls, generic substitutions, and explicit closure captures.
Synchronous and async reference execution are implemented; production code generation,
native ABI support, and the application platform remain future work; draft
specifications describe more than is implemented today.

## Development and validation

```sh
cargo fmt --all --check
cargo build --workspace
cargo clippy --workspace --all-targets --all-features
cargo test --workspace
```

Use `cargo test -p <crate>` for focused checks. Invalid frontend fixtures declare
expected diagnostic codes using `// expect: CODE`; selected diagnostics also have
`.stderr` snapshots. Formatter fixtures check layout, idempotence, AST equivalence,
and comment preservation.

For syntax or editor-query changes, run `python3 tooling/tree-sitter-zen/check.py`
with Python 3, Node.js, and Tree-sitter CLI 0.26.3 available. See the
[grammar guide](tooling/tree-sitter-zen/README.md) and
[Zed validation instructions](editors/zed/README.md#checks) for setup and the separate
extension build check, which is outside the root Cargo workspace.

## Documentation

- [Type system and grammar](docs/type-system-and-grammar.md): precise core rules;
  takes precedence over the broader [language specification](docs/spec.md).
- [Implementation notes](docs/implementation-notes.md): decisions, ambiguities,
  compiler APIs, source-root rules, and current frontend limits.
- [Typed HIR](docs/hir.md): representation contract and boundaries for lowering work.
- [Reference interpreter](docs/interpreter.md): execution semantics, APIs, limitations,
  and [conformance fixtures](tests/conformance/README.md).
- [Canonical formatting](docs/formatting.md): style, APIs, and regression testing.
- [Zed integration](editors/zed/README.md) and
  [Tree-sitter grammar](tooling/tree-sitter-zen/README.md): editor setup and checks.
- [Contributor instructions](AGENTS.md): architecture boundaries, working practices,
  validation, and documentation upkeep.

Keep this README and the relevant guides current in the same change as features,
commands, setup requirements, architecture, or limitations evolve. Add focused
documentation for new topics and link it from the relevant guide.

## License

The MIT license text is available in [LICENSE.md](LICENSE.md). Cargo package
metadata currently declares `MIT OR Apache-2.0`.
