# Execution conformance

These ordinary Zen programs describe observable synchronous and async behavior for the
reference interpreter and future backends. Each `.zen` has a `.expected` containing
the expected returned value (`Unit`, `Int(14)`, `Bool(true)` or `String("text")`).
Programs testing complex values return primitive observations, avoiding backend
layout or compilation-specific nominal IDs in expectations. Assertions are
ordinary source expressions; no new Zen syntax or standard library is assumed.

Optional `.host` files explicitly link a native declaration to a test operation:

```text
record trace-int
```

`trace-int` takes one Int, records its canonical decimal representation, and
returns Unit. An accompanying `.events` contains the exact ordered records, one
per line. Empty/missing events means no records. Linkage is performed outside
execution using the compiled declaration identity; the evaluator dispatches only
on semantic IDs. This is a conformance host contract, not a new prelude function
or production native binding. `zen run` deliberately has no test host, so use a
fixture without `.host` for a standalone CLI example.

`compiler/crates/zen-interpreter/tests/conformance.rs` discovers fixtures recursively
in sorted order, parses/checks/lowers them, drops frontend data, and executes twice
with fresh hosts. It compares both returned values and events exactly. Run:

```sh
cargo test -p zen-interpreter --test conformance
cargo run -p zen-cli -- check tests/conformance/functions/calls.zen
cargo run -p zen-cli -- fmt --check tests/conformance/functions/calls.zen
cargo run -p zen-cli -- run tests/conformance/functions/calls.zen
```

Keep expectations explicit and review changes. Do not regenerate expected results
from the implementation under test. A future backend adapter must compare the
same semantic values and host observations, not Rust debug layouts. Failure tests
for malformed HIR or reference-specific errors live in the interpreter's Rust
integration tests, outside the cross-backend language contract.

The `async/` fixtures cover tasks as ordinary values, async functions/lambdas,
propagation, generic/interface environments, suspension within expression and
control-flow forms, and lexical defer cleanup. Event expectations establish
source evaluation order and cleanup across awaited dependencies, without observing
unawaited task start timing. FIFO scheduling, repeated awaits, task identity,
cancellation, and deadlocks are reference-executor unit tests, not cross-backend
contracts until the language specifies those policies.
