# Working on Zen

This file applies to the entire repository. Keep it accurate as the project evolves.

## Project context

Zen is a statically typed language and planned cross-platform application
platform focused on predictable semantics and reliable generated code. The
repository currently implements a compiler frontend, typed HIR, a reference
interpreter with deterministic async tasks, a canonical formatter, a language
server, and Zed integration. Production code generation and platform runtimes
remain future work. Do not describe planned runtime, platform, or
standard-library features as implemented.

The compiler is a Rust 2024 workspace using the stable toolchain configured in
`rust-toolchain.toml`. The core frontend uses the standard library; the
interpreter uses num-bigint and stacker; the LSP uses Tokio and
tower-lsp-server. Preserve the lightweight core dependency boundary.

## Read these first

- [README](README.md): project overview and quick-start commands.
- [Type system and grammar](docs/type-system-and-grammar.md): precise core-language
  rules; this document takes precedence over the broader language specification.
- [Language specification](docs/spec.md): overall language design and intent.
- [Implementation notes](docs/implementation-notes.md): draft ambiguities,
  implementation decisions, compiler APIs, source roots, and current limitations.
  Read later entries for decisions that supersede earlier milestone limitations.
- [Canonical formatting](docs/formatting.md): style, formatter invariants, CLI and
  editor behavior, and regression-test conventions.
- [Typed HIR](docs/hir.md): representation contract, lowering API, semantic identities,
  golden tests, and boundaries for future compiler stages.
- [Reference interpreter](docs/interpreter.md): execution model, host boundary,
  entry points, errors, and [conformance fixtures](tests/conformance/README.md).
- [Zed integration](editors/zed/README.md): installation, capabilities, architecture,
  validation, and troubleshooting.
- [Tree-sitter grammar](tooling/tree-sitter-zen/README.md): syntax tooling and checks.

Before changing language behavior, consult the precise grammar and relevant
implementation notes. Do not silently resolve a spec conflict by inventing syntax
or semantics. Record the issue, chosen behavior, rationale, and any required spec
follow-up in the implementation notes using their existing format.

## Repository structure

| Path | Responsibility |
| --- | --- |
| `compiler/crates/zen-diagnostics/` | Source maps, spans, diagnostic codes, structured diagnostics, and rendering. |
| `compiler/crates/zen-syntax/` | Lexer, parser, AST, recovery, trivia, and syntax-role metadata. |
| `compiler/crates/zen-semantics/` | Resolution, nominal types, type checking, and shared IDE analysis. `src/checker/` divides checking by concern. |
| `compiler/crates/zen-hir/` | Owned typed/resolved high-level IR, lowering, deterministic dumps, and fixture tests for future backends. |
| `compiler/crates/zen-interpreter/` | HIR evaluator, deterministic executor, task continuations, values, frames/scopes, and execution tests. |
| `compiler/crates/zen-format/` | Canonical formatting over compiler syntax, document rendering, and formatter fixtures/property tests. |
| `compiler/crates/zen-lsp/` | LSP transport, workspace snapshots, position conversion, and editor features backed by compiler analysis. |
| `compiler/crates/zen-cli/` | The `zen` binary: `check`, `run`, `fmt`, and `lsp`, plus CLI/LSP integration tests. |
| `tests/conformance/` | Shared execution programs, expected return values, and optional host observations. |
| `tests/fixtures/valid/` | Programs the frontend must accept. |
| `tests/fixtures/invalid/` | Programs it must reject, expected diagnostic codes, and selected `.stderr` snapshots. |
| `tooling/tree-sitter-zen/` | Syntax-only editor grammar, corpus tests, and grammar/query validation. |
| `editors/zed/` | Separate Rust extension crate, language configuration, queries, and local development preparation script. |
| `examples/editor/` | Small cross-file example for exercising editor integration. |
| `docs/` | Language specifications, implementation decisions, and feature documentation. |
| `target/` | Build outputs and generated local editor/grammar artifacts; not source to edit or commit. |

## Implementation practices

- Read the affected code and tests before editing. Keep changes focused, preserve
  unrelated work, and follow the surrounding Rust conventions. Add dependencies
  only when their benefit justifies expanding the dependency surface.
- Keep parsing, semantic analysis, formatting, and transport responsibilities in
  their owning crates. CLI and editor features should reuse compiler APIs; do not
  introduce a second resolver or type checker in the LSP, extension, or grammar.
- Keep Tree-sitter syntax and Zed queries aligned with compiler syntax when grammar
  changes. Tree-sitter is for editing structure, not authoritative type checking.
- Preserve deterministic behavior, source spans, stable diagnostic codes, and
  structured type information. Handle malformed input with diagnostics or recovery,
  not panics; maintain bounded handling of deeply nested input.
- Treat `Type::Error` as recovery-only. Normal compilation stops on syntax errors;
  IDE analysis may recover partial programs, but downstream lowering must require
  an error-free program. Never reuse symbol or nominal IDs across analysis snapshots.
- Keep CLI and LSP source-root/import behavior consistent through shared APIs.
  The current root is the nearest ancestor named `src`, or the file's directory.
  Do not assume a package manifest or registry exists.
- For LSP work, preserve immutable snapshots, document versions, stale-result
  rejection, and correct UTF-16 position conversion. Keep CPU-heavy work off the
  async executor and logs off protocol stdout.
- Preserve formatter determinism, idempotence, semantic structure, literal spelling,
  and comment order. Use compiler syntax facts rather than guessing with text
  substitutions. Never bulk-format intentionally invalid or unformatted fixtures.
- Add focused regression coverage for behavior changes and bug fixes, including
  rejection/recovery cases where relevant. Review snapshot changes rather than
  regenerating expectations merely to make a failure disappear.

## Validation

Run commands from the repository root unless a linked guide says otherwise.
During development, run the affected crate's tests with `cargo test -p <crate>`.
Before completing Rust implementation changes, run the workspace checks:

```sh
cargo fmt --all --check
cargo build --workspace
cargo clippy --workspace --all-targets --all-features
cargo test --workspace
```

For syntax, Tree-sitter, or Zed query changes, also run the following with Node.js,
Python 3, and Tree-sitter CLI 0.26.3 available (see the linked setup guides):

```sh
python3 tooling/tree-sitter-zen/check.py
```

The Zed extension is outside the root workspace. For extension Rust changes, also
check it separately with its target installed:

```sh
cargo fmt --manifest-path editors/zed/Cargo.toml --check
cargo check --manifest-path editors/zed/Cargo.toml --target wasm32-unknown-unknown
```

Invalid frontend fixtures use `// expect: CODE` lines; the harness rejects missing
and unexpected codes. Deliberate diagnostic snapshot updates use
`ZEN_UPDATE_SNAPSHOTS=1 cargo test -p zen-semantics --test fixtures`; inspect the diff.
Formatter cases belong in paired `*.input.zen` / `*.expected.zen` fixtures under
`compiler/crates/zen-format/tests/fixtures/`, whose harness checks layout,
idempotence, AST equivalence, and comment preservation.

Use the Zed guide for manual editor checks when integration behavior changes.
For documentation-only work, verify facts, commands, and relative links; compiler
tests are unnecessary unless behavior also changes. Report what was actually
validated and any checks that could not run.

HIR cases use `.zen` / `.hir` pairs in `compiler/crates/zen-hir/tests/fixtures`.
Update deliberately with `ZEN_UPDATE_HIR=1 cargo test -p zen-hir --test lowering
golden_dumps` and inspect the snapshots. Lowering consumes error-free semantic
data; keep resolution decisions in `zen-semantics`, and runtime/layout choices
out of HIR.

Interpreter execution consumes HIR only; never execute syntax or redo resolution.
Execution fixtures use `.zen` / `.expected` pairs under `tests/conformance`; optional
`.host` and `.events` files declare test-host bindings and observations. Keep native
bridges keyed by semantic IDs. Run `cargo test -p zen-interpreter` for execution
changes and preserve evaluation order, value captures, and lexical defer cleanup.
Task continuations live only in the interpreter, not HIR. Scheduling and cancellation
policy tests belong in executor unit tests; cross-backend async fixtures must not
prescribe unspecified task start timing, repeated-await or ownership policy.

## Keep documentation current as you work

Documentation is part of implementation, not a follow-up task. At each meaningful
implementation step, review this file, the root README, and the affected docs and update them in the
same change when behavior, structure, workflows, or limitations have changed. Do
not defer documentation until the end of a feature or wait for a separate request.

- Always update [README.md](README.md) in the same change whenever relevant project
  information changes: capabilities, implementation status, commands, setup or
  dependencies, repository structure, validation, limitations, or documentation
  entry points. Check README impact for every task; do not wait for a separate
  request. Keep its overview concise and link to detailed guides rather than
  duplicating them. Clearly distinguish work in progress from completed features.
- Document every new feature, subsystem, public API, workflow, or significant design
  decision. Extend the owning document for small additions; create a focused
  `docs/<topic>.md` or subsystem README when the topic needs its own explanation.
- New documentation should explain purpose, where the implementation lives,
  behavior and usage, important constraints, and how to validate it. Distinguish
  implemented behavior from planned work and unresolved decisions.
- Link new docs from the relevant existing guide or root README, and add them to
  this file's reading list when future contributors need them. Avoid orphan docs
  and duplicated explanations; keep detailed guidance in its owning document.
- Update this file whenever crates/directories, architecture boundaries, development
  commands, testing conventions, or contributor practices change. Keep it a concise
  project map and working guide rather than a chronological change log.
- Correct or explicitly supersede outdated claims as implementations advance.
  Keep examples, commands, capability lists, and current limitations synchronized
  with code; preserve historical rationale only when clearly labeled as historical.
- Before finishing, explicitly check README accuracy, review the diff for
  documentation impact, verify local links,
  and ensure no new behavior or setup requirement is left undocumented. Do not
  claim completion while required documentation updates remain outstanding.
