# Zen for Zed

The extension uses Tree-sitter for editing structure and launches the compiler's
`zen lsp` over stdio for semantic features. It contains no resolver or checker.

## Install locally

From the repository root, with stable Rust, Python 3, Node.js, and Tree-sitter CLI
0.26.3 available:

```sh
cargo install --path compiler/crates/zen-cli --locked
# Only if tree-sitter is not installed:
npm install --prefix tooling/tree-sitter-zen
export PATH="$PWD/tooling/tree-sitter-zen/node_modules/.bin:$PATH"
python3 editors/zed/scripts/prepare-dev.py
```

Ensure `$HOME/.cargo/bin` is on your shell's `PATH`. Verify `zen --version`.
In Zed's command palette, run **zed: install dev extension** and select
`<repository>/target/zed-dev` (on this machine:
`/Users/ahmedghoneim/zen/target/zed-dev`). Zed builds the Rust extension and grammar;
its first build may download the official WASI SDK. Open
`examples/editor/src/main.zen`. The status bar should say **Zen**.

Select **`target/zed-dev`**, not `editors/zed`. The source directory's manifest
contains a placeholder grammar URL and cannot be installed directly. If Zed
reports “failed to compile grammar 'zen'” with “failed to fetch revision HEAD”,
rerun the preparation script above, then install `target/zed-dev`.

The preparation script generates the parser, makes a local Git grammar snapshot
under `target/tree-sitter-zen-dev`, and writes a development extension with a
`file://` grammar URL and pinned revision. This works even when the compiler and
grammar are uncommitted. It does not change the repository's Git history. After
syntax/query changes, rerun the script and use **zed: rebuild dev extension**. Keep
`target/zed-dev` and `target/tree-sitter-zen-dev` while using the dev extension.

After compiler changes, repeat `cargo install --path compiler/crates/zen-cli --locked`
and run **editor: restart language server**. There is no toolchain downloader.

## Verify and troubleshoot

- Hover `user` in the example: `User`, after both await and propagation.
- Go to its type definition: `models.zen` → `User`.
- Type `user.` inside the function: field and applicable method completion.
- Remove a match arm: compiler diagnostic and “Add missing match arms”.
- Use the buffer outline, matching brackets, and Vim function/class text objects.
- Run **dev: open language server logs** and select **Zen** for protocol logs;
  **zed: open log** includes the binary path and startup failures.
- Run `RUST_LOG=debug zen lsp` for tracing on stderr. Stdout is protocol-only.
- If formatting does nothing after installing or rebuilding the extension, focus
  a `.zen` file and run **editor: restart language server**, then **editor: format**.
  Zed can stop the existing Zen server during extension reload without starting
  its replacement for an already-open buffer.

The repository's `.zed/settings.json` enables semantic tokens for Zen. In other
projects, add `"languages": { "Zen": { "semantic_tokens": "combined" } }` to Zed
settings. Inlay hints follow the user's Zed inlay-hint setting.

## Capabilities and boundaries

Diagnostics, hover, definitions/type definitions, references, rename, symbols,
highlights, implementations, completion, signature help, full semantic tokens,
inferred-local inlay hints, and diagnostic quick fixes use compiler queries.
Completion covers lexical scope, static members (including constrained and
collection methods), contextual enums, missing match variants, type contexts,
and named argument labels. Parsing is tolerant of unfinished blocks.

Rename rechecks a speculative in-memory workspace and verifies reference targets;
it requires an error-free starting program. Aliased-import renames are deliberately
rejected when they cannot be represented as a single symbol rename. Enum/method
navigation has no fabricated source locations for compiler builtins.

Missing-arm actions insert syntactically valid TODO blocks. Fill these in before
running code; a value-producing match can still require a result in each new arm.
Whole-document formatting uses the shared canonical formatter. Range formatting
is intentionally not advertised. Import-path completion, documentation comments, parameter
inlay hints, and semantic-token deltas are not implemented. Deeply broken syntax
can leave a feature with no usable semantic context; it returns no result.

Source roots follow `zen check`: nearest ancestor named `src`, otherwise the
file's directory. No package manifest or dependency registry is invented. Each
root is analyzed as a small workspace; open buffers override disk text. File
watching is registered when supported by the client. IDs are snapshot-scoped,
and rename edits carry open-document versions. CPU work runs off the async
executor; stale generations are discarded.

## Formatting

Reinstall the updated CLI and restart the language server after compiler changes.
Use **Format Document** (called **editor: format** in the current command palette)
on a `.zen` buffer. Zed sends the unsaved text through `zen lsp` to `zen-format`.
No external formatter command or extension-specific formatting code is needed.

Zed's default `"formatter": "auto"` selects the Zen language server when no other
formatter overrides it. To select it explicitly and enable format-on-save, add
this to your project or user settings:

```json
{
  "languages": {
    "Zen": {
      "formatter": "language_server",
      "format_on_save": "on"
    }
  }
}
```

The formatter always uses Zen's canonical four-space/100-column style, independent
of editor indentation options. Invalid syntax produces no formatting edits.
Selection/range formatting is deferred; use whole-document formatting, and use
`"on"` rather than `"modifications"` for format-on-save.

The same formatter is available outside Zed:

```sh
zen fmt src/main.zen
zen fmt .
zen fmt --check .
cat src/main.zen | zen fmt -
```

See [canonical style and CLI behavior](../../docs/formatting.md). Current API
references: [formatting settings](https://zed.dev/docs/configuring-languages#formatting-and-linting),
[formatter selection](https://zed.dev/docs/reference/all-settings#formatter), and
[language metadata](https://zed.dev/docs/extensions/languages). Formatting is
selected by editor settings and LSP capabilities, not a formatter field in the
extension's language metadata.

Manual verification (2026-09-18): a scratch project using the development `zen`
binary passed Format Document, save followed by `zen fmt --check`, explicit
language-server format-on-save, and `auto` format-on-save. Arabic/emoji line
comments and trailing/block comments survived. No global settings were changed.

## Checks

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features
cargo test --workspace
cargo check --manifest-path editors/zed/Cargo.toml --target wasm32-unknown-unknown
python3 tooling/tree-sitter-zen/check.py
```

API references checked for this implementation: [Zed language extensions](https://zed.dev/docs/extensions/languages),
[manifest schema](https://github.com/zed-industries/zed/blob/main/crates/extension/src/extension_manifest.rs),
[Zed Rust API 0.7](https://docs.rs/zed_extension_api/0.7.0/zed_extension_api/), and
[tower-lsp-server 0.23](https://docs.rs/tower-lsp-server/0.23.0/tower_lsp_server/).

## Validation on 2026-09-18

38 Rust tests and 44 Tree-sitter corpus cases pass, along with workspace Clippy,
Rust formatting, all valid compiler fixtures through Tree-sitter, and all Zed
query compilations. The extension builds for `wasm32-unknown-unknown`.

Loaded as a development extension in the local Zed installation. Confirmed syntax
highlighting, buffer outline, automatic bracket insertion/closing-brace indentation,
`user: User` hover after await/propagation, cross-file type navigation, live
`ZEN-TYPE-0001` diagnostics on an unsaved `Float = 10`, and `user.` completion
showing `name` and `describe`. Temporary test edits were undone. Vim text-object
queries compile; Vim keystroke behavior was not separately exercised.

A synthetic 500-local function took approximately 0.73 seconds in the unoptimized
debug CLI on the development machine. This is a baseline, not an interactive
latency guarantee; scope snapshot storage and whole-root analysis are candidates
for future incremental optimization.
