# Canonical Zen formatting

Zen has one local, deterministic, offline formatter, shared by `zen fmt` and
`zen lsp`. There is no formatter configuration file. Formatting changes
presentation, not names, imports, types, evaluation order, or expression binding.

```sh
zen fmt src/main.zen              # rewrite only if changed
zen fmt src/foo.zen src/bar.zen    # multiple files or directories
zen fmt .                        # recursively format .zen sources
zen fmt --check .                # CI: list changes without writing
cat src/main.zen | zen fmt -      # source only on stdout; diagnostics on stderr
```

Exit status is 0 for success, 1 for files needing formatting in check mode, and
2 for syntax, formatter, argument, or filesystem errors. Each invalid file remains
untouched; other valid files can still be formatted. Replacements use a temporary
file in the same directory, preserve permissions, and avoid rewriting unchanged
files. Directory traversal is sorted and deduplicated. It skips symlinks, hidden
entries, `target`, `build`, `dist`, `node_modules`, `vendor`, `generated`, and
`__generated__`. No package manifest or dependency registry is assumed; pass the
package directory or `src` to restrict the scope. Do not run an in-place formatter
across intentionally ugly or invalid test fixtures.

## Style

- Four spaces, LF line endings, no generated trailing whitespace, one final newline.
- Preferred width of 100 Unicode scalar values (an approximation of display width).
  Indivisible tokens and preserved comments can exceed that width.
- Opening braces stay with the preceding header. All brace bodies, including empty
  blocks, struct declarations/literals/updates, enum bodies and lambdas, are multiline.
- One struct field or enum variant per line. Struct literals and updates have
  trailing commas. Declaration fields and enum variants retain semicolons.
- Short calls, parameters, generic lists, function types, patterns and list literals
  stay inline. Wrapped lists put one item per line and include a trailing comma.
  A sole lambda argument can hug the call parentheses, as in `users.map((user) {`.
- Return arrows and types stay after the closing parameter delimiter. Long binary
  expressions break before operators with one continuation indent; wrapped member
  chains break before dots. Postfix `?` remains attached.
- One space around binary operators and after colons; no spaces inside member
  access or after symbolic unary operators. Literal spelling and explicit
  parentheses are preserved, including around `await` and propagation.
- One blank line between top-level declarations and method definitions. Imports
  remain in their original order, one per line, with a blank line before declarations.
  A block match arm is followed by a blank line unless it is the last arm. Simple
  arms are adjacent. Required arm semicolons are preserved.
- Arbitrary blank lines are discarded. Before comments, at most one intentional
  blank line survives, in addition to structural declaration/method spacing.

## Comments and invalid input

Comments are neither removed nor reflowed. Same-line comments trail the preceding
token; other comments lead the following token or dangle inside a closing delimiter.
Their relative token order is preserved, even in unusual positions. Literal bytes
are unchanged. Comment prose and block-comment interior indentation are preserved;
CRLF becomes LF and trailing line-comment whitespace is removed. Internal whitespace
inside block comments is treated as comment content, not generated indentation.

Only syntactically valid core Zen is formatted. Parser diagnostics abort formatting;
type errors do not prevent it. There is no lint fixing, import sorting, semantic
analysis, Tree-sitter parsing, or AI dependency in the formatter.

## Editors

Zed uses `textDocument/formatting` from `zen lsp`; see the
[Zed setup](../editors/zed/README.md#formatting). The server formats the current
unsaved buffer on a worker, checks its version/text before returning, and returns
one replacement edit with a UTF-16 document range. Already formatted buffers return
no edits; syntax errors return no result and retain existing diagnostics.

Range formatting is intentionally deferred and is not advertised. Arbitrary
substrings are not complete syntax nodes and cannot be formatted safely using the
whole-file API. Use Format Document or whole-document format-on-save.

## Implementation and regression tests

`zen-syntax::parse_source` returns the normal AST plus tokens with leading trivia
spans and parser-recorded syntax roles. EOF owns the file's trailing trivia. The
parser records generic/initializer/grouping spans, operator roles, lambdas, and
statement/declaration boundaries; it does not choose indentation or line counts.
`parse` remains compatible with existing compiler callers.

`zen-format` builds a document from those concrete syntax facts and original token
spelling, then renders it. Its document algebra implements text, soft/hard lines,
concatenation, groups, indentation, forced groups and conditional trailing commas.
It depends only on syntax and diagnostics. `format_source`, `format_file` (with a
caller's `FileId`), and `format_parsed` are reusable in-memory APIs. The latter
requires the exact source used to produce the parse result.

Add paired `*.input.zen` / `*.expected.zen` files under
`compiler/crates/zen-format/tests/fixtures`. Every pair automatically checks exact
layout, idempotence, AST equality after recursively clearing spans and expression
IDs, and comment preservation/order. Tests also cover the compiler fixture corpus,
generated operator combinations, comments injected at token boundaries, whitespace
variants, exact width boundaries, the renderer, CLI writes/check/stdin, and the LSP.
