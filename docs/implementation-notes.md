# Frontend implementation notes

The original language documents are unchanged. The compiler now includes typed
HIR after the frontend (see the later HIR entries), but not a complete standard
library, native binding validator, or runtime.

## 2026-09-18 — Grammar precedence and statement blocks

**Spec issue:** §74 explicitly makes `await operation()?` mean
`(await operation())?`, while the generic precedence grammar would imply the
opposite. The grammar does not place `with` in its precedence table. Examples
also omit semicolons after statement-position `if` and `match` blocks.

**Implementation choice:** Calls and members bind above `await`, followed by
propagation, then other unary and binary operations. `with` is reserved and is a
low-precedence expression suffix; its update must contain at least one field.
Block-shaped statements may omit their terminator. Match arms still require
semicolons. An `if` without `else` in statement position is a statement even at
the end of a block; it cannot initialize a value, including `Unit`.

**Reasoning:** Follow explicit normative resolutions and preserve the distinction
between statements and value-producing expressions. No `try` prefix is accepted.

**Needs spec update:** yes — reconcile EBNF with these explicit examples.

## 2026-09-18 — Generic expressions and control-flow braces

**Spec issue:** The draft does not disambiguate `<` as comparison versus generic
arguments, or struct-literal braces versus control-flow bodies.

**Implementation choice:** A syntactically complete type-argument list is accepted
in expression position when followed by a call, member, struct initializer, or
expression delimiter. Other cases remain comparisons. Struct literals in an
`if`/`while` condition or `match`/`for` scrutinee require parentheses. Generic
struct arguments may be inferred from a matching contextual type or their fields.
Interface methods follow the precise EBNF, which has no method generic parameters.

**Reasoning:** Deterministic syntax without capitalization-based heuristics.

**Needs spec update:** yes.

## 2026-09-18 — Package roots and native declarations

**Spec issue:** The package manifest and native binding layers are not yet defined.

**Implementation choice:** The CLI uses the nearest ancestor named `src` as its
source root, or the input file's directory otherwise. `import a.b.Name` loads
`<root>/a/b.zen` and imports public declaration `Name`; aliases are supported.
Imports are loaded transitively and cycles are visited once. The semantic API
also accepts a caller-supplied collection of named modules. There is no package
registry or dependency discovery. Native signatures receive ordinary core type
checking; platform-specific ABI eligibility is deferred until a binding layer
exists, and successful checking does not certify a native implementation.

**Reasoning:** Useful local modules without inventing a package format or ABI.

**Needs spec update:** yes — package and native specifications are still needed.

## 2026-09-18 — Operators and prelude boundary

**Spec issue:** The drafts describe some library APIs without complete signatures
and leave exact ordering operand types open. `Float` and `F64` are described as
equivalent. Struct defaults appear in the broad spec but not the precise grammar.

**Implementation choice:** `F64` canonicalizes to `Float`. Compiler arithmetic is
`+`, `-`, `*`, unary negation for `Int`/floats, plus string concatenation. Ordering
currently accepts numeric types only. `/` and `%` are not syntax; fixed-width
arithmetic operators are rejected. Signed minimum literals such as `-128i8` are
range-checked as one literal value. Unsuffixed integers retain their text and
have no machine-integer limit. Struct fields have no defaults.

The prelude owns `Option`, `Result`, `Task`, `List`, `Map`, and `Set` definitions.
It supplies the documented `List.get`, `List.first`, `List.append`, and `Map.get`
contracts. Iteration accepts `List` and `Set`. Other library APIs, including
checked arithmetic, hashing, and collection builders, require future declarations;
unknown APIs are diagnosed. Equality is structural for structs/enums; functions,
tasks, and collections without specified equality contracts are rejected.

**Reasoning:** Enforce only contracts with known static semantics. No wrapping,
implicit numeric conversion, or guessed fallible API is introduced.

**Needs spec update:** yes — finish operator and standard-library contracts.

## 2026-09-18 — Conservative proof boundaries

**Spec issue:** Constant evaluation, default-expression purity, named-function
block-tail returns, and compiler resource limits are not fully specified.

**Implementation choice:** Named functions and lambdas require explicit returns
on reachable non-`Unit` paths; block tail values remain available inside other
expressions. Constant evaluation accepts literals, pure operators, aggregate
literals, and acyclic constant references. Defaults additionally accept earlier
parameters; calls whose purity cannot be established are rejected. General
compile-time function evaluation is deferred. Loop analysis conservatively
assumes a loop can finish; all bodies and match arms are checked, including
unreachable ones. Exhaustiveness uses recursive pattern-matrix specialization,
so a partially covered payload does not cover its entire enum variant.

Recursive-descent and expression checking enforce bounded nesting (32 nested
checking frames / operator-chain steps). Equality and exhaustiveness proofs also
have conservative depth limits. Exceeding a limit produces a diagnostic. These
are implementation resource limits, not additional language semantics.

**Reasoning:** Missing proof must not become an implicit runtime check or an
accepted unsafe program. Limits protect the frontend from stack exhaustion.

**Needs spec update:** yes — specify constant/default evaluation and function tails.

## Compiler API and tests

`zen_syntax::parse` returns an AST and structured diagnostics. Callers should stop
before semantic checking if syntax errors exist. `zen_semantics::check` returns
source AST modules, a nominal-definition table, symbols, and typed expressions
keyed by `(FileId, ExprId)`, retained resolution decisions for HIR lowering,
as well as diagnostics. `Type::Error` is recovery-only;
semantic output is usable for later lowering only when no errors remain. Symbol
and nominal IDs are stable within one checked program, not persisted across runs.

Diagnostic codes are independent of prose. Fixture expectations reject both
missing and unexpected codes. Selected `.stderr` snapshots can be deliberately
updated with `ZEN_UPDATE_SNAPSHOTS=1 cargo test -p zen-semantics --test fixtures`.
CLI tests cover exit status, module loading, visibility, and invalid UTF-8;
frontend tests also exercise deterministic malformed input and nesting limits.

## 2026-09-18 — Editor analysis and unfinished programs

**Spec issue:** There is no canonical formatter or specified placeholder expression
for unfinished value-producing match arms. `state` is reserved by the core keyword
list even though some informal editor examples use it as a local name.

**Implementation choice:** Preserve the keyword rule; examples use `status`.
Missing-arm quick fixes insert empty blocks with explicit TODO comments (valid
syntax, potentially incomplete typing), never an invented `todo`/`fatal` builtin.
Formatting stays unadvertised until a canonical formatter exists. Parser recovery
retains an unfinished block's AST while still reporting its missing closing brace.

**Reasoning:** Interactive tools must tolerate incomplete input without changing
valid-program semantics or inventing runtime behavior.

**Needs spec update:** yes — define formatter/placeholder conventions separately.

## IDE analysis API

`zen_semantics::analysis::Database` accepts in-memory files with stable slots and
explicit module names. `analyze` returns a snapshot of `CheckedProgram`, including
compiler-resolved declaration/reference identities, lexical environments, expected
types, calls, implementation links, and inferred local types. Partial syntax is
checked for IDE recovery, but lowering still requires an error-free program.
Symbols/nominals belong to one snapshot; never retain these IDs across analyses.
The LSP keeps snapshots immutable, versions text edits, and rejects stale results.
`source_root` is shared by the CLI and LSP. No editor resolver is introduced.


## 2026-09-18 — Canonical formatter and grammar fidelity

**Spec issue:** Older examples omit match-arm semicolons, imply `try` syntax or
platform-only constructs, and show several equivalent optional statement
terminators. The precise grammar allows trailing commas in parameter, generic,
payload, initializer, and argument lists. There is no package manifest yet.

**Implementation choice:** The formatter accepts only the existing compiler's
valid syntax and preserves explicit parentheses and statement terminators.
Match arms retain their mandatory semicolons, including block arms. No grammar
or precedence rule changes. Literal spelling is retained, avoiding escape or
numeric normalization. Formatting uses one canonical style documented in
`formatting.md`, superseding the earlier unadvertised-formatter limitation.
Directory formatting visits explicitly requested trees with documented exclusions;
it does not invent package or generated-source metadata. Range formatting is
intentionally deferred until complete syntax-region replacement is supported.

**Reasoning:** Reconcile presentation with the actual parser without introducing
semantic transformations. Leading trivia spans and parser syntax-role metadata
are sufficient; a separate lossless parser and Tree-sitter dependency are unnecessary.

**Needs spec update:** yes — reconcile older examples with the precise match-arm
and await grammar, as recorded above. No new language ambiguity was resolved by
changing semantics in this milestone.

## 2026-09-19 — Typed HIR and retained semantic decisions

**Spec issue:** The frontend previously retained types and IDE references but
not all substitutions, parameter mappings or member identities needed by later
compiler stages. `TypeId` denotes nominal declarations, not arbitrary interned
types. Default evaluation timing remains unspecified beyond deterministic,
declaration-context expressions and references to earlier parameters.

**Implementation choice:** Add `zen-semantics::resolved` checker facts and the
owned `zen-hir` lowering API. Reuse canonical `Type`, `SymbolId` and nominal IDs;
introduce owner/index field and variant IDs. Retain structured control flow,
interface requirements versus concrete implementations, generic substitutions,
explicit captures, async completion types, and classified propagation. Omitted
defaults reference their declaration parameter; they are not materialized at call
sites. Iteration retains the existing List/Set contract. This supersedes the
frontend-only pipeline description; there is still no execution or backend.
See [typed HIR](hir.md) for the contract, API, tests and deferred work.

**Reasoning:** Downstream consumers need resolved meaning without repeating the
checker or committing to runtime layout/control-flow choices. Retaining defaults
separately avoids imposing an unstated evaluation rule.

**Needs spec update:** yes — settle default evaluation timing before execution.
No accepted source syntax or checking rules were changed by this milestone.

## 2026-09-19 — Divergent operands at the HIR boundary

**Spec issue:** The checker accepts `await` and `?` on `Never`, even though no
Task/Option/Result value can be produced. It also accepts a `Never` callee without
checking its unreachable argument syntax, unlike its usual policy of checking
unreachable code.

**Implementation choice:** HIR records a typed `Diverge` operation that evaluates
the checked operand and cannot proceed. For a Never callee the argument syntax
has no checked semantic value and is not carried into HIR. Other checked
unreachable code remains present. Existing frontend acceptance is unchanged.

**Reasoning:** There is no callable, propagation kind or task completion type to
invent when evaluation cannot reach the operation. Backend execution must not
attempt invocation or evaluate arguments after the callee diverges.

**Needs spec update:** yes — decide whether such calls should instead diagnose
unreachable argument syntax in a future checker change.
