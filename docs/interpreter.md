# Reference interpreter

`zen-interpreter` executes successfully checked, owned typed HIR, including
async tasks. It is an executable reference for language semantics, not the
production runtime. The CLI pipeline is source loading → parsing/checking →
`zen_hir::lower_program` → interpretation. Invalid source never executes. The
evaluator has no direct syntax, formatter, LSP or editor dependency and performs
no name resolution, inference or method lookup. `num-bigint` and host stack
growth (`stacker`) are confined to execution; the core frontend remains
standard-library-only. Building stacker requires a native C build toolchain
(such as Xcode Command Line Tools on macOS).

## Running a program

```sh
cargo run -p zen-cli -- run tests/conformance/functions/calls.zen
zen run path/to/main.zen
```

The entry file must declare a top-level, parameterless, non-generic `main`
returning `Unit`, `Int`, `Bool` or `String`. Public visibility is optional.
Imported functions and methods named `main` are not entry points. These are
reference-tool entry rules, not permanent platform application semantics. An
async main creates a root task and drives it to completion. A synchronous main
returning `Task<T>` remains outside these entry rules and is rejected.

Success exits 0, source/I/O/interpreter errors exit 1, and invalid CLI usage exits 2.
Returned values are available through the library API; `zen run` neither prints
them nor treats integers or booleans as process exit codes. No console native is
implicitly installed. Imports and frontend diagnostics are identical to `zen check`.

## Library and host boundary

```rust,ignore
let hir = zen_hir::lower_program(&checked)?;
drop(checked);
let mut interpreter = zen_interpreter::Interpreter::new(&hir, zen_interpreter::NoHost);
let value = interpreter.run_main(entry_module)?;
// Or invoke an already-resolved function with all its arguments:
let value = interpreter.run_function(function_id, arguments)?;
```

`run_function_with_types` accepts explicit `ParamId → Type` substitutions for
embedding generic calls. The caller supplies checked HIR and correctly typed
arguments from that program. Values/callables and semantic IDs belong to that HIR
snapshot and must not be reused with another program. The interpreter borrows
HIR, caches constants per instance, and retains no checked AST lifetime.

`Host::call_native` is an optional reference-native bridge keyed by `SymbolId`.
It receives substitutions and parameter-order values, returning `None` for an
unbound declaration, or `Some(Result<Value, String>)`. Hosts must honor checked
signatures. This supports deterministic test observations without giving console
output a special evaluator role. An embedder can hold output buffers in its
host; `interpreter.host` exposes that state. The CLI uses `NoHost`. Async native
declarations use the same bridge for immediate completion values; unbound
declarations fault when driven. There is no general asynchronous native FFI,
external I/O polling, native ABI or platform API.

`InterpreterError` distinguishes missing/invalid entry, unbound native,
cancellation ambiguity, deadlock, host failure, constant cycles, resource
exhaustion and internal invariant failures. It retains an expression span and an
outer-to-inner stack of semantic function/lambda/constant identities, debug
names and call-site spans. `render(&SourceMap)` provides source locations.
`Option.none` and `Result.err` are ordinary values, never interpreter errors.

## Values and operations

Values are safely owned Rust data with explicit `Unit`. Integers are arbitrary
precision; fixed-width values retain their checked type and exact integer value.
The currently available `+`, `-`, `*` and negation apply to `Int` and IEEE `Float`/
`F32`, with string concatenation for `+`. There is no fixed-width arithmetic,
wrapping, division or checked numeric API in current HIR. Floating NaNs and
infinities remain valid values; NaN equality and ordering follow IEEE semantics.
Strings are immutable Unicode, characters are scalar values, and no indexing API
is invented. Struct/enum equality is structural; callable and collection equality
without a frontend contract is an invariant error.

Structs use nominal IDs and declaration-indexed fields. Enums, including Option
and Result, use `VariantId` and ordered payloads. List values share immutable
storage; append and struct update produce new values. Closures snapshot captures
by local ID and retain their generic substitution environment. No mutable outer
slot or Rust closure is exposed to Zen. Ordinary recursion uses distinct explicit
frames. `Value` display is deterministic developer output, not a serialization ABI.

The four existing HIR intrinsics are `ListGet`, `ListFirst`, `ListAppend` and
`MapGet`. Negative, excessive or arbitrary-precision out-of-range list indices
return `.none`. No names are compared to select an intrinsic. Source has list
literals but no Map/Set constructors yet. Embedders may supply ordered Map/Set
values; map lookup compares keys structurally, and Set iteration follows supplied
order. Host Set values must be unique and Map keys unique and equality-capable.
This host convention does not prescribe a future source-level collection API.

## Evaluation and control flow

Receiver/callee, arguments, operands, list elements, enum payloads and struct
initializers evaluate left to right. Named argument expressions retain source
order and are then bound using HIR parameter indices. Omitted defaults run after
all supplied expressions, in declaration order, in the declaration's parameter
and generic context. Requirement defaults use requirement-local identities before
values are rebound to the selected implementation's parameters. Defaults can read
earlier parameters; the frontend restricts them to deterministic expressions.

Generic functions execute without monomorphization. Requirement calls use the
substituted concrete receiver type, instantiated interface and requirement ID to
select an already-checked implementation link. Concrete calls invoke their resolved
method directly. This is identity-based witness selection, not source method lookup.

Blocks have explicit lexical scopes with local slots and defer stacks. A defer
registers its expression only when reached and evaluates it at exit, reading the
current local values. All scope exits use the same reverse-order cleanup path:
normal completion, return, propagation, break and continue. The return expression
or block tail is computed before cleanup; later rebinding cannot alter that value.
The checker rejects returns, propagation and loop exits that cross a deferred
expression boundary (`ZEN-TYPE-0042`); cleanup-local loops and nested lambda exits
remain valid. Loops consume only exits for their resolved loop ID. Match evaluates the scrutinee
once and binds only the selected arm's immutable locals.

Interpreter errors also attempt registered cleanup; the first error is retained
while remaining defers are attempted. This is a reference-tool recovery policy,
not a Zen exception mechanism. Async calls produce task handles; suspension
preserves active scopes. Merely declaring an unused native function does not
prevent execution.

The recursive evaluator uses `stacker` to grow the host stack before recursive
evaluation, including on small Rust test/worker stacks. It has an implementation
guard at 512 active expression evaluations per task (including synchronous calls
and constant dependencies), reported as `ResourceLimit` with a Zen stack. It is
not a language recursion limit. Process memory exhaustion and
long-running/infinite loops are not converted into Zen failures; there is no
instruction budget or preemption. Internal cancellation is cooperative at
suspension boundaries. Infinite synchronous computation cannot be interrupted.

## Tests

```sh
cargo test -p zen-interpreter
cargo test -p zen-cli
cargo fmt --all --check
cargo build --workspace
cargo clippy --workspace --all-targets --all-features
cargo test --workspace
```

[Execution conformance fixtures](../tests/conformance/README.md) contain ordinary
Zen source and explicit expected returned values, with optional declared host
observations. The harness checks, lowers, drops the checked AST, and executes each
fixture twice with fresh interpreters. Additional tests cover interpreter errors,
malformed HIR invariants, host failures, stack/source retention, constant cycles,
embedding and CLI import/diagnostic/exit behavior. Future backends should consume
the same source and expectations rather than adopting interpreter internals.

## Deterministic task execution

`src/executor.rs` owns stable, interpreter-local `TaskId`s, a task table, and a FIFO
ready queue. `Value::Task` is an identity handle, not a Rust future. Task values can
travel through ordinary aggregates, parameters, returns, lists, and closure
captures. Task equality remains unsupported. `TaskId::index`, `task_state`, and
`task_dump` provide developer diagnostics; dumps are never printed automatically.
Handles must not be reused across interpreter instances, even for the same HIR.

The reference policy starts a task when first awaited or explicitly driven.
Arguments/defaults are evaluated at the call, and the bound frame is retained in
the created task. Multiple observers reuse the retained completion value. These
are provisional implementation choices, not new language guarantees; see the
[implementation notes](implementation-notes.md). Lifecycle states are Created,
Ready, Running, Waiting(target), Completed, Cancelled, and Faulted. Completed
Option.none/Result.err values remain successful completion. Waiters wake in
registration order, and duplicate/terminal wakeups are ignored.

`src/eval.rs` retains the original structured evaluator and cleanup paths as
boxed internal Rust continuations. Each task owns its frames, mutable locals,
generic environment, lexical scopes, and partially evaluated expressions.
Continuation polling grows the host stack where needed; async recursion across
tasks does not grow a native call chain. Await only suspends when its target is
incomplete. Rust wakers do not schedule Zen work, and dropping a Rust future is
never the cancellation mechanism. Native host calls are serviced and resumed
inline without switching Zen tasks. No new dependency or external executor is
required. Constants remain synchronous and cached per interpreter.

Library `run_function` and `run_function_with_types` preserve ordinary call
semantics: async invocations return `Value::Task(id)`. `drive_task(id)` returns its
completion, and permits repeated observation. For example:

```rust,ignore
let Value::Task(task) = interpreter.run_function(async_function_id, vec![])? else {
    unreachable!("checked async function");
};
let completion = interpreter.drive_task(task)?;
interpreter.shutdown()?;
```

`run_main` automatically drives async main and shuts down pending work on both
success and failure. Embedders can keep task values between library calls and
explicitly call `shutdown` to observe cleanup errors. Drop also attempts shutdown,
but cannot report errors. Completed task records/values remain available until
interpreter destruction. No task runs outside an explicit call into the executor.

Each task records its creator, creation scope, children, waiters, and debug name.
The internal cancellation mechanism can request cancellation of a task and its
creation descendants. Lexical scope exit does not automatically cancel escaped
task values: ownership transfer policy remains unresolved. Cancellation resumes a
suspended continuation with an internal executor cancellation signal, unwinding
through existing lexical cleanup paths. Registered defers run exactly once in
reverse order; deferred code may itself await. Unstarted bodies have no registered
cleanup. Cleanup faults replace cancellation as the reported failure. Cancellation
is neither a Zen exception nor a fabricated Result error. Awaiting a cancelled
task reports `UnspecifiedCancellation` until the language defines that result.

Self-await and an incomplete root without runnable work report `Deadlock`.
If deferred cleanup deadlocks during shutdown, the executor fails the blocked
await and resumes unwinding so remaining registered defers are still attempted.
No-progress errors include a deterministic task dump; child faults retain source
frames and acquire the awaiting frames as they propagate. There are no threads,
timers, networking, detached execution, source cancellation/spawn APIs, or
production platform runtime. Internal scheduler tests exercise real suspension,
multiple waiters, cancellation cleanup, cycles, terminal-state invariants, and
stable traces independently of cross-backend conformance.
