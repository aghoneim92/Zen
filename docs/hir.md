# Typed HIR

HIR is the backend-neutral, owned representation after successful parsing,
resolution and checking. `zen-hir` depends on compiler semantics, syntax (only
for lowering), and diagnostics. Its public nodes contain no AST or tokens.
The formatter and LSP continue consuming their existing frontend APIs.

## Contract

Lowering must reject programs with diagnostics and report missing semantic facts
as internal lowering errors with source spans. Every expression, block and
pattern carries the canonical semantic `Type`; `TypeId` identifies a nominal
definition, not an interned arbitrary type. The resulting program owns nominal
metadata and does not borrow its checked input. IDs belong to this compilation.

Local references and assignments use `SymbolId`. Fields and variants use an
owner nominal ID plus declaration-order index. Calls retain resolved targets,
explicit generic substitutions, and parameter indices. No backend performs name,
field, variant, method, import or pattern lookup, inference, or overload selection.
Operators carry a semantic operation and checked operand type. Contextual enum
syntax disappears. Imports have no executable representation.

Evaluation order is source order: receiver/callee before arguments, left before
right, and field initializers, payloads and list elements in their original order.
Named arguments carry destination indices without being sorted. Short circuit
operators remain distinct from eager operations. Source spans survive lowering.

Generic declarations and interface constraints survive without monomorphization.
Concrete implementations and generic interface requirements remain distinct.
`Option`/`Result` remain ordinary nominal enums; propagation is explicitly
classified. Await retains its typed task operand and completion result.

Blocks, if, match, loops, return and defer remain structured. Loop exits have
resolved loop IDs. Defer belongs to its enclosing lexical block and must run in
reverse registration order on all exits (including propagation and loop exits).
Iteration retains the current compiler-known List/Set semantics; no iterator
protocol is invented. Lambdas have explicit parameters and typed value captures,
including outer locals needed to create nested lambdas. No closure environment
layout is chosen.

HIR itself does not execute constants, defaults or programs. The [reference
interpreter](interpreter.md) executes synchronous and async HIR while retaining
suspension state internally. MIR control flow, match decision trees, defer
expansion, async state machines, closure conversion, monomorphization,
optimization, memory layout, linking and production native ABI/runtime behavior
belong to later stages.

## API and ownership

```rust,ignore
let checked = zen_semantics::check(modules); // stop on parse errors first
let hir = zen_hir::lower_program(&checked)?;
let dump = hir.dump();
drop(checked); // HIR has no syntax-tree lifetime
```

`analysis::Database::analyze` also works as input and includes parse diagnostics.
`LoweringError::InvalidProgram` rejects frontend diagnostics;
`LoweringError::Internal { span, message }` reports a violated compiler invariant.
The low-level `check` API cannot recover parse diagnostics a caller discarded.

`Program` owns modules, nominal definitions, generic constraints, functions,
constants, implementation links, local metadata and an expression arena. Arena
IDs are stable for this HIR program; semantic IDs remain those of its checked
snapshot. `Module` uses the existing `FileId`. Builtin nominals have no source
module. Field/variant indices index their owner's declaration-order metadata.
Names in these records are debug metadata only. Functions distinguish a Zen body,
a native declaration, and an interface requirement, and retain async completion
type separately from the `Task<T>` type of calls.

`zen-semantics::resolved` stores checker decisions at their original source keys:
local identities, field/variant indices, pattern types, callable targets, argument
mapping and substitutions. Its declaration bodies still use AST internally;
`zen-hir` consumes them once. No IDE query heuristics or second resolver is used.

## Calls and defaults

Resolved call targets distinguish ordinary/native functions, inherent methods,
concrete interface implementations, generic interface requirements and the four
existing collection intrinsics. A requirement includes its instantiated interface
and receiver parameter type; a concrete implementation links its method to the
interface requirement. An indirect call instead evaluates a function expression.
A bound method value stores its receiver and substitutions explicitly.

Call arguments are stored in evaluation order, with zero-based destination
parameter indices in the full declaration signature (including `self` at index 0
for bound methods). The receiver is separate and evaluated first. Function-value
arguments index the function type's parameters, which exclude a bound receiver.
Resolved substitutions are `(ParamId, Type)` pairs; nominal type arguments also
remain in `Type::Nominal`. Applying known substitutions is not type inference.

An omitted default is a parameter index in `Call.defaults`. Its one typed
initializer remains on the declaration's `Parameter.default`. Earlier-parameter
references use that declaration's local IDs. Defaults are not copied into callers,
constant-folded, or evaluated here. An executor must bind supplied argument
values, apply the call's substitutions, and use declaration-context defaults for
omitted parameters. The reference interpreter evaluates supplied expressions
first in source order,
then omitted defaults in declaration order. This execution decision is recorded
in the implementation notes; HIR continues to retain the distinction explicitly.
Interface requirement defaults belong to the requirement's
signature and its instantiated interface context.

## Literals and divergence

Strings and characters reuse the lexer's decoded values. Arbitrary-precision
integers use a sign plus canonical decimal magnitude (no suffix, separators or
leading zeros), retaining all digits without a bigint dependency. Fixed-width
integer types are already range-checked; a signed minimum literal is represented
as one negative value. Floats are decoded into their declared IEEE precision.

The current checker accepts `Never` operands of await/propagation, and accepts a
`Never` callee without checking its unreachable argument syntax. HIR uses
`Diverge { operand }` for these cases: evaluating the operand cannot reach an
await, unwrap or invocation. Unchecked, unreachable argument syntax is not a HIR
value. This narrowly mirrors an existing frontend boundary; ordinary unreachable
checked statements and expressions are retained, and no general dead-code pass
runs. See the implementation note for the pending checker/spec decision.

## Tests and dumps

Run `cargo test -p zen-hir`. Tests lower every valid frontend fixture, reject
invalid analysis, and cover ordering, literals, identity, methods, inference,
constraints, defaults, captures, propagation, async, loops, all supported primitive
operator families and cross-module ownership. `.zen`/`.hir` pairs under
`compiler/crates/zen-hir/tests/fixtures` are deterministic golden dumps. The dump
prints metadata and each typed arena expression with its ID and source span;
child IDs describe structured control flow, not an eager execution schedule.
Update deliberately with `ZEN_UPDATE_HIR=1 cargo test -p zen-hir --test lowering
golden_dumps`, then inspect every changed snapshot. The debug dump is a developer
API, not a stable wire format or an additional CLI command.
