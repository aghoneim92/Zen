# tree-sitter-zen

Syntax-only grammar for the implemented Zen core. The compiler owns resolution
and typing. The grammar follows `docs/type-system-and-grammar.md` and the
precedence clarifications in `docs/implementation-notes.md`: notably
`await operation()?` is propagation of the awaited result.

Use Tree-sitter CLI 0.26.3 and Node.js:

```sh
tree-sitter generate
tree-sitter test
# From repository root, also validate compiler fixtures and Zed queries:
python3 tooling/tree-sitter-zen/check.py
```

Generated C/parser metadata under `src/` is ignored; the development extension
preparation script regenerates and copies it into a local grammar Git snapshot.
Corpus files are reviewed syntax snapshots, including incomplete editor input.
The Zed query files live in `editors/zed/languages/zen` to avoid divergent copies.
Angle brackets are matched only inside generic parameter/type-argument nodes;
comparison operators are never general auto-closing bracket pairs.
