# Zen Core Language
## Type System and Grammar Specification

**Status:** Draft 0.1  
**Scope:** Core language syntax, static semantics, and type checking  
**Companion document:** Zen Language Specification, Draft 0.1

---

# 1. Purpose

This document defines the core syntax and static semantics of Zen.

It is intended to be precise enough to guide implementation of:

- the parser;
- the AST;
- name resolution;
- the type checker;
- exhaustiveness checking;
- semantic diagnostics;
- formatter behavior;
- language-server tooling.

The Zen UI framework, component model, networking APIs, location APIs, storage APIs, and other platform facilities are specified separately.

The core language is intentionally independent of any particular application framework.

---

# 2. Normative principles

When two possible interpretations of Zen syntax or semantics exist, the implementation must prefer the interpretation consistent with these principles:

1. A syntactically valid program has one unambiguous parse.
2. A successfully typed expression has one statically known type.
3. No implicit operation may introduce a runtime type failure.
4. No implicit conversion changes a value's type.
5. No overload-resolution ranking is performed.
6. Failure is represented explicitly in types.
7. Native code is the explicit boundary of Zen's safety guarantees.
8. The same source-level semantics apply on every target.
9. Public APIs expose their complete static types.
10. Local inference must never change API meaning.

---

# 3. Safety model

Safe Zen guarantees that ordinary language operations cannot produce:

- null dereferences;
- invalid casts;
- uninitialized reads;
- array or list bounds exceptions;
- integer overflow for `Int`;
- unchecked fixed-width integer overflow;
- uncaught language exceptions;
- use-after-free;
- dangling references;
- data races through ordinary Zen state;
- invalid enum discriminants;
- calling a value that is not callable;
- accessing a member that does not exist.

Zen does not guarantee program termination.

The following are outside the normal language-safety guarantee:

- exhaustion of physical memory;
- exhaustion of operating-system resources;
- forced process termination;
- explicit calls to `fatal`;
- defects inside native code;
- defects inside the Zen runtime implementation itself.

These cases must never create undefined language semantics.

---

# 4. Source encoding

Zen source files are UTF-8.

Source files use the extension:

```text
.zen
```

String contents support full Unicode.

Identifiers in Zen 1 are deliberately restricted to ASCII.

```ebnf
identifier =
    identifier_start,
    { identifier_continue };

identifier_start =
    "A"…"Z"
  | "a"…"z"
  | "_";

identifier_continue =
    identifier_start
  | "0"…"9";
```

This restriction exists to:

- avoid visually confusable identifiers;
- simplify compiler tooling;
- improve generated-code reliability;
- produce predictable symbol names across platforms.

A future language version may extend identifier syntax.

---

# 5. Keywords

The following identifiers are reserved:

```text
as
async
await
break
component
const
continue
defer
else
enum
false
fn
for
if
impl
import
in
interface
let
match
native
public
return
self
state
struct
true
var
view
while
```

The following literals are also reserved:

```text
unit
```

`component`, `state`, and `view` are reserved for the Zen platform specification even though they are not otherwise defined by this core specification.

There is no `null` keyword.

There is no `throw` keyword.

There is no `try` keyword.

---

# 6. Comments

Single-line comments begin with `//`.

```zen
// This is a comment.
```

Block comments use:

```zen
/*
This is a block comment.
*/
```

Block comments may not nest in Zen 1.

Comments have no semantic effect.

---

# 7. Statement termination

Semicolons are mandatory after statements that syntactically require them.

There is no automatic semicolon insertion.

```zen
let name = "Zen";
process(name);
```

This is invalid:

```zen
let name = "Zen"
process(name)
```

Mandatory statement terminators reduce parser ambiguity and make generated code more deterministic.

---

# 8. Modules

Every source file defines exactly one module.

The module name is derived from its path relative to the package's source root.

```text
src/models/user.zen
```

defines:

```text
models.user
```

Zen source files do not contain explicit module declarations.

---

# 9. Imports

A declaration may be imported using its complete path.

```zen
import app.models.User;
import zen.http.Http;
```

An imported declaration may be renamed:

```zen
import company.auth.User as AuthUser;
```

Wildcard imports do not exist.

The following is invalid:

```zen
import app.models.*;
```

Two imported declarations may not introduce the same local name unless one is explicitly renamed.

Imports do not execute runtime code.

---

# 10. Visibility

Module-level declarations are private by default.

```zen
struct InternalState {
}
```

A public declaration uses `public`.

```zen
public struct User {
}
```

Struct fields are also private by default.

```zen
public struct User {
    public id: Int;
    public name: String;

    internalValue: String;
}
```

Interface methods are always part of the interface's visible contract and do not individually use `public`.

---

# 11. Core types

Zen defines these core types:

```text
Bool
Int
Float
String
Char
Unit
Never
```

It also defines fixed-width numeric types:

```text
I8
I16
I32
I64

U8
U16
U32
U64

F32
F64
```

`Float` is equivalent in representation and semantics to `F64`, but `Float` is the canonical application-level spelling.

`Int` is the canonical application-level integer type.

---

# 12. Type grammar

```ebnf
type =
    named_type
  | function_type;

named_type =
    qualified_identifier,
    [ type_arguments ];

type_arguments =
    "<",
    type,
    { ",", type },
    [ "," ],
    ">";

function_type =
    "(",
    [ type, { ",", type }, [ "," ] ],
    ")",
    "->",
    type;

qualified_identifier =
    identifier,
    { ".", identifier };
```

Examples:

```zen
String
User
List<User>
Map<String, User>
Result<User, NetworkError>
(Int, String) -> Bool
() -> Unit
```

Zen 1 does not define tuple types.

The parentheses in function types describe function parameters rather than tuples.

---

# 13. Nominal typing

Structs and enums are nominal types.

Two declarations with identical structure are still different types.

```zen
struct UserId {
    public value: String;
}

struct OrderId {
    public value: String;
}
```

`UserId` is not assignable to `OrderId`.

No structural conversion exists between them.

---

# 14. Type equality

Except for `Never`, assignment and argument passing normally require exact static type equality.

For example:

```zen
let value: Int = 10;
```

is valid.

This is invalid:

```zen
let value: Float = 10;
```

because Zen does not implicitly convert `Int` to `Float`.

Generic types are invariant.

Therefore:

```text
Container<Child>
```

is not implicitly assignable to:

```text
Container<Parent>
```

even if some relationship exists between `Child` and `Parent`.

---

# 15. Subtyping

Zen 1 has deliberately minimal subtyping.

The universal subtype rule is:

```text
Never <: T
```

for every type `T`.

There is no general class hierarchy.

Interface conformance is a constraint relation, not ordinary nominal inheritance.

Function types do not participate in implicit variance-based subtyping in Zen 1.

---

# 16. The `Never` type

`Never` represents an expression that does not return normally.

If:

```text
Γ ⊢ e : Never
```

then `e` may appear in any context expecting a type `T`.

Examples include:

```zen
fatal("unreachable")
```

and compiler-proven non-returning operations.

---

# 17. Unit

`Unit` has exactly one value:

```zen
unit
```

A function returning `Unit` may omit an explicit `return`.

```zen
fn log(message: String) -> Unit {
    Console.write(message);
}
```

An explicit return is also valid:

```zen
return unit;
```

or:

```zen
return;
```

inside a `Unit`-returning function.

---

# 18. Boolean values

Boolean literals are:

```zen
true
false
```

Conditional expressions require exactly `Bool`.

Zen has no truthiness conversion.

This is invalid:

```zen
if users {
}
```

even if `users` is a list.

---

# 19. Integer literals

An unsuffixed integer literal has type `Int`.

```zen
42
-12
1_000_000
```

Numeric separators are ignored semantically.

Fixed-width literals use explicit suffixes.

```zen
42i32
255u8
100i64
```

A suffixed literal is a compile-time error when its value cannot be represented by its declared type.

Therefore:

```zen
256u8
```

does not compile.

---

# 20. Float literals

An unsuffixed floating-point literal has type `Float`.

```zen
3.14
1.0
0.5
```

Explicit fixed-width forms include:

```zen
3.14f32
3.14f64
```

Scientific notation is supported:

```zen
1.5e10
2.0e-4
```

---

# 21. `Int` arithmetic

`Int` is an arbitrary-precision signed integer.

The following operations cannot overflow:

```text
+
-
*
unary -
```

Therefore:

```zen
let total = a + b;
```

has type `Int` when `a` and `b` are `Int`.

Integer division can fail when the divisor is zero.

Zen therefore does not define `/` as an unchecked `Int -> Int` operation.

Integer division uses an explicit checked API:

```zen
a.div(b)
```

with a type equivalent to:

```text
Result<Int, ArithmeticError>
```

The same principle applies to integer remainder.

---

# 22. Fixed-width integer arithmetic

Fixed-width integers exist primarily for:

- native APIs;
- binary formats;
- protocols;
- performance-sensitive low-level libraries.

Arithmetic operators that could overflow are not defined directly for fixed-width integers.

Instead:

```zen
I32.checkedAdd(a, b)
I32.checkedSubtract(a, b)
I32.checkedMultiply(a, b)
```

produce:

```text
Result<I32, ArithmeticError>
```

Wrapping arithmetic must always be requested explicitly.

```zen
I32.wrappingAdd(a, b)
```

Application code should normally use `Int`.

---

# 23. Floating-point arithmetic

Floating-point operations follow IEEE-754 semantics.

Division by zero may produce infinity or NaN rather than a language error.

These are valid `Float` values.

Zen does not silently convert floating-point values to integers.

---

# 24. String literals

Strings use double quotes.

```zen
"Hello"
```

Supported escapes include:

```text
\\
\"
\n
\r
\t
\u{...}
```

`String` is immutable Unicode text.

Integer indexing of strings is invalid.

```zen
text[3]
```

does not exist in Zen 1.

Unicode-aware access is provided by standard-library APIs.

---

# 25. Characters

A `Char` represents one Unicode scalar value.

Examples:

```zen
'A'
'λ'
```

A user-perceived grapheme may contain multiple `Char` values.

Applications performing user-visible text manipulation should generally use grapheme-aware string APIs.

---

# 26. Variable declarations

An immutable local binding uses `let`.

```zen
let name = "Zen";
```

A mutable local binding uses `var`.

```zen
var count = 0;
count = count + 1;
```

Both forms require initialization.

These are invalid:

```zen
let value;
var value;
```

Zen has no uninitialized local variables.

---

# 27. Type annotations

A local binding may provide an explicit type.

```zen
let name: String = "Zen";
```

The initializer must be assignable to the declared type.

No conversion is attempted.

---

# 28. Assignment

Only bindings declared with `var` may be reassigned.

```zen
var count = 0;
count = count + 1;
```

This is invalid:

```zen
let count = 0;
count = 1;
```

Struct fields are immutable values.

Declaring:

```zen
var user = User {
    name: "Ahmed",
};
```

does not make `user.name` directly mutable.

This is invalid:

```zen
user.name = "Ali";
```

Instead:

```zen
user = user with {
    name: "Ali",
};
```

---

# 29. Shadowing

Zen 1 does not allow local variable shadowing.

This is invalid:

```zen
let value = 10;

if condition {
    let value = 20;
}
```

A local declaration may not reuse the name of:

- a parameter;
- another local in an enclosing scope;
- another local in its own scope.

This restriction intentionally reduces ambiguity in generated and transformed code.

---

# 30. Struct declarations

```ebnf
struct_declaration =
    [ "public" ],
    "struct",
    identifier,
    [ generic_parameters ],
    "{",
    { struct_field },
    "}";

struct_field =
    [ "public" ],
    identifier,
    ":",
    type,
    ";";
```

Example:

```zen
public struct User {
    public id: Int;
    public name: String;
}
```

Every field must contain a valid value for its declared type.

There are no uninitialized fields.

---

# 31. Struct construction

Struct construction names every field.

```zen
let user = User {
    id: 1,
    name: "Ahmed",
};
```

Field order is not semantically significant.

Unknown fields are compile-time errors.

Missing required fields are compile-time errors.

Duplicate fields are compile-time errors.

---

# 32. Immutable struct updates

A struct value may be copied with selected fields changed.

```zen
let updated = user with {
    name: "Ali",
};
```

The type of `updated` is the same nominal type as `user`.

All replacement values must exactly match their field types.

The original value remains unchanged.

---

# 33. Enum declarations

```ebnf
enum_declaration =
    [ "public" ],
    "enum",
    identifier,
    [ generic_parameters ],
    "{",
    { enum_variant },
    "}";

enum_variant =
    identifier,
    [ "(", type, { ",", type }, [ "," ], ")" ],
    ";";
```

Example:

```zen
enum LoadState<T> {
    idle;
    loading;
    loaded(T);
    failed(AppError);
}
```

Enums are closed.

The set of variants is fixed by their declaration.

---

# 34. Enum construction

The fully explicit form is:

```zen
LoadState<User>.loaded(user)
```

A contextual shorthand is allowed when the expected enum type is statically known:

```zen
return .loaded(user);
```

The shorthand is invalid when more than one enum type could satisfy the expression.

Generated public API examples should prefer the explicit form when context is not immediately obvious.

---

# 35. Option

`Option<T>` is a predeclared enum equivalent to:

```zen
enum Option<T> {
    some(T);
    none;
}
```

Absence is always represented with `Option`.

There is no nullable `T`.

---

# 36. Result

`Result<T, E>` is a predeclared enum equivalent to:

```zen
enum Result<T, E> {
    ok(T);
    err(E);
}
```

Recoverable failure is represented using `Result`.

Zen does not have ordinary language exceptions.

---

# 37. Generic declarations

```ebnf
generic_parameters =
    "<",
    generic_parameter,
    { ",", generic_parameter },
    [ "," ],
    ">";

generic_parameter =
    identifier,
    [ ":", interface_constraint ];

interface_constraint =
    named_type,
    { "&", named_type };
```

Example:

```zen
fn encode<T: Serializable & Equatable>(
    value: T
) -> String {
    ...
}
```

Generic type parameters are nominal placeholders.

Zen 1 does not support:

- higher-kinded type parameters;
- generic parameter defaults;
- variance declarations;
- higher-ranked generic functions;
- dependent types.

---

# 38. Interfaces

An interface defines required behavior.

```ebnf
interface_declaration =
    [ "public" ],
    "interface",
    identifier,
    [ generic_parameters ],
    "{",
    { interface_method },
    "}";

interface_method =
    [ "async" ],
    "fn",
    identifier,
    "(",
    parameter_list,
    ")",
    "->",
    type,
    ";";
```

Example:

```zen
interface Serializable {
    fn serialize(self) -> String;
}
```

Interfaces do not contain stored state.

Interfaces do not inherit implementation.

Zen 1 does not support associated types.

---

# 39. Implementations

An interface implementation is explicit.

```zen
impl Serializable for User {
    fn serialize(self) -> String {
        return Json.encode(self);
    }
}
```

An implementation must provide exactly the required method signatures.

Parameter and return types must match exactly.

There is no overload matching.

---

# 40. Implementation coherence

For any concrete type and concrete interface instantiation, at most one implementation may exist within a program.

Zen rejects ambiguous implementations at compile time.

Packages may not introduce competing implementations that cause call resolution to depend on import order.

Importing a module never changes which implementation is selected.

---

# 41. Inherent methods

Types may define methods without an interface.

```zen
impl User {
    fn displayName(self) -> String {
        return self.name;
    }
}
```

An inherent implementation may only be declared in the module that defines the nominal type.

This prevents arbitrary extension methods from changing global method resolution.

---

# 42. `self`

Methods explicitly declare `self`.

```zen
fn displayName(self) -> String
```

There is no invisible receiver parameter.

`self` is immutable.

Zen 1 does not support `mut self`.

Types that expose controlled mutation do so through explicit APIs such as framework state or native handles.

---

# 43. Method resolution

For:

```zen
value.method(argument)
```

the compiler performs deterministic lookup:

1. Find an inherent method named `method`.
2. Otherwise find a uniquely applicable interface method from an interface implementation already known from the static type and constraints.
3. If zero candidates exist, report an error.
4. If more than one candidate would apply, report an ambiguity error.

Zen never ranks overload candidates.

There may not be two inherent methods with the same name on one nominal type.

---

# 44. Function declarations

```ebnf
function_declaration =
    [ "public" ],
    [ "async" ],
    "fn",
    identifier,
    [ generic_parameters ],
    "(",
    parameter_list,
    ")",
    "->",
    type,
    block;

parameter_list =
    [ parameter, { ",", parameter }, [ "," ] ];

parameter =
    identifier,
    ":",
    type,
    [ "=", expression ];
```

Example:

```zen
fn greet(name: String) -> String {
    return "Hello, " + name;
}
```

Every named function requires:

- parameter types;
- an explicit return type.

Function body inference never changes a public or private function's declared signature.

---

# 45. Function overloading

Function overloading does not exist.

This is invalid:

```zen
fn parse(value: String) -> Int {
    ...
}

fn parse(value: String) -> Float {
    ...
}
```

Use:

```zen
parseInt(value)
parseFloat(value)
```

instead.

This rule also applies to methods.

---

# 46. Function calls

Calls may use positional arguments:

```zen
request(url, timeout);
```

or named arguments:

```zen
request(
    url: url,
    timeout: timeout,
);
```

A call may not mix positional and named arguments.

This is invalid:

```zen
request(url, timeout: timeout);
```

Named argument labels must exactly match declared parameter names.

---

# 47. Default parameters

Parameters may provide default values.

```zen
fn request(
    url: Url,
    timeout: Duration = Duration.seconds(30),
) -> Result<Response, HttpError> {
    ...
}
```

With positional calling, omitted parameters must form a trailing sequence.

With named calling, any parameter with a default may be omitted.

Default expressions are type checked in the function's declaration context.

They may not depend on later parameters.

---

# 48. Function values

Functions are first-class values.

```zen
let transform: (Int) -> String = intToString;
```

A function value's type includes:

- every parameter type;
- its return type.

Parameter names are not part of the function type.

---

# 49. Lambdas

A lambda uses:

```zen
(value: Int) -> Int {
    return value * 2;
}
```

When the lambda occurs in a context with a known function type, parameter and return type annotations may be omitted.

```zen
numbers.map((value) {
    return value * 2;
});
```

If the context does not provide sufficient information, annotations are required.

Zen does not perform global lambda-type inference.

---

# 50. Closure capture

Closures capture local values.

Captured values cannot be assigned to through the closure.

This is invalid:

```zen
var count = 0;

let increment = () -> Unit {
    count = count + 1;
};
```

Mutable state that must outlive a call scope must use an explicit state-holding abstraction.

This rule prevents hidden shared mutable bindings.

---

# 51. Constants

Module constants use `const`.

```zen
public const defaultTimeout: Int = 30;
```

Constant initializers must be compile-time evaluable.

Module-level mutable variables do not exist in Zen 1.

---

# 52. Native declarations

A native function declaration has no Zen body.

```zen
native fn platformName() -> String;
```

An asynchronous native function is:

```zen
native async fn currentLocation()
    -> Result<Location, LocationError>;
```

Only types supported by the active native binding layer may appear across a native boundary.

Unsupported native signatures are compile-time errors.

A native declaration is an explicit trust boundary.

---

# 53. Type inference

Zen uses local, bidirectional type inference.

The compiler may infer:

- local variable types;
- generic arguments;
- contextually typed lambda parameters;
- contextual enum constructor types.

The compiler may not infer:

- named function parameter types;
- named function return types;
- public field types;
- enum payload types;
- exported API signatures.

Inference does not search arbitrarily across unrelated modules.

---

# 54. Generic inference

Given:

```zen
fn identity<T>(value: T) -> T {
    return value;
}
```

the call:

```zen
identity(10)
```

infers:

```text
T = Int
```

When generic arguments cannot be uniquely inferred, they must be supplied explicitly.

```zen
createEmptyList<String>()
```

A generic inference ambiguity is a compile-time error.

The compiler does not select an arbitrary solution.

---

# 55. No implicit conversions

Zen performs no implicit conversion between distinct types.

This includes:

```text
Int -> Float
Float -> Int
I32 -> Int
Int -> I32
String -> UserId
T -> Option<T>
T -> Result<T, E>
```

Every such conversion must be explicit.

A compile-time numeric literal with an explicit suffix is not considered an implicit conversion.

---

# 56. Casts

Unchecked casts do not exist.

Potentially invalid runtime conversions must return:

```text
Option<T>
```

or:

```text
Result<T, E>
```

depending on whether diagnostic failure information is useful.

Native binding layers may internally perform platform-specific conversions, but their Zen-facing APIs must expose safe results.

---

# 57. Expression typing notation

This specification uses:

```text
Γ ⊢ e : T
```

to mean:

> Under type environment `Γ`, expression `e` has type `T`.

Assignment compatibility is written:

```text
T1 ≼ T2
```

and means:

> A value of type `T1` may be used where `T2` is required.

In Zen 1:

```text
T ≼ T
```

and:

```text
Never ≼ T
```

for all `T`.

There are no other universal assignment conversions.

---

# 58. Local bindings

For:

```zen
let x: T = e;
```

the program is valid when:

```text
Γ ⊢ e : S
S ≼ T
```

The environment after the declaration contains:

```text
x : T
```

as an immutable binding.

For:

```zen
let x = e;
```

if:

```text
Γ ⊢ e : T
```

then `x` receives type `T`.

---

# 59. Mutable assignment

Given:

```text
x : var T
```

an assignment:

```zen
x = e;
```

is valid only when:

```text
Γ ⊢ e : S
S ≼ T
```

Assignment is a statement and has no value usable by another expression.

Chained assignment does not exist.

---

# 60. Return typing

For a function declared:

```zen
fn f(...) -> T
```

every:

```zen
return e;
```

must satisfy:

```text
Γ ⊢ e : S
S ≼ T
```

A bare:

```zen
return;
```

is valid only when the declared return type is `Unit`.

Every reachable path through a function returning a type other than `Unit` must return a value.

---

# 61. If statements

```zen
if condition {
    ...
} else {
    ...
}
```

requires:

```text
Γ ⊢ condition : Bool
```

There is no conversion to `Bool`.

`else` is optional when the `if` is used only as a statement.

---

# 62. If expressions

`if` may produce a value.

```zen
let title = if authenticated {
    "Home"
} else {
    "Sign in"
};
```

An `if` used as a value requires an `else`.

If one branch has type `A` and the other has type `B`, there must be a single type `T` such that:

```text
A ≼ T
B ≼ T
```

Given Zen's minimal subtyping, this normally means:

```text
A = B
```

except where one branch is `Never`.

---

# 63. Blocks

A block contains zero or more statements and may end with a final expression without a semicolon.

```zen
{
    let x = compute();

    x + 1
}
```

The block's type is the type of its final expression.

A block with no final expression has type `Unit`.

A semicolon converts an expression into an expression statement, so:

```zen
{
    value;
}
```

has type `Unit`.

The canonical formatter must make this distinction visually clear.

---

# 64. Match expressions

```zen
match value {
    .some(item) => use(item);
    .none => fallback();
}
```

The scrutinee is evaluated exactly once.

Each arm is type checked in its own pattern-binding scope.

When `match` is used as a value, all reachable arms must produce assignment-compatible types.

---

# 65. Match exhaustiveness

A `match` over an enum must cover:

- every enum variant; or
- a wildcard `_`.

A `match` over `Bool` must cover:

```text
true
false
```

or include `_`.

A match over an open value domain such as `Int` or `String` must contain `_` unless the compiler can otherwise prove exhaustiveness.

Failure to prove exhaustiveness is a compile-time error.

---

# 66. Patterns

Zen 1 patterns include:

```ebnf
pattern =
    "_"
  | identifier
  | literal_pattern
  | enum_pattern;

enum_pattern =
    [ "." | qualified_identifier, "." ],
    identifier,
    [ "(", pattern, { ",", pattern }, [ "," ], ")" ];
```

Examples:

```zen
.some(value)
.none
Result.ok(value)
_
true
42
```

Pattern guards are not part of Zen 1.

Or-patterns are not part of Zen 1.

---

# 67. Pattern bindings

A binding introduced by a pattern is immutable.

```zen
.some(user)
```

introduces:

```text
user : T
```

within that match arm.

Pattern bindings may not shadow an existing local name.

---

# 68. Error propagation with `?`

The postfix `?` operator propagates `Option` and `Result`.

For:

```text
e : Result<T, E>
```

inside a function returning:

```text
Result<U, E>
```

the expression:

```zen
e?
```

has type:

```text
T
```

If `e` evaluates to:

```text
Result.err(error)
```

the current function immediately returns:

```text
Result.err(error)
```

---

# 69. Result error types

Zen performs no implicit error conversion during `?`.

Therefore this does not compile:

```text
e : Result<User, NetworkError>

current function:
Result<Profile, AppError>
```

unless the error is explicitly converted first.

Correct code uses something such as:

```zen
let user = loadUser(id)
    .mapError((error) {
        return AppError.network(error);
    })?;
```

This rule avoids hidden `From`-style conversions.

---

# 70. Option propagation

For:

```text
e : Option<T>
```

inside a function returning:

```text
Option<U>
```

the expression:

```zen
e?
```

has type `T`.

If `e` is `.none`, the current function immediately returns `.none`.

`Option` propagation cannot occur from a function returning `Result`, or vice versa, without explicit conversion.

---

# 71. `?` restrictions

`?` may appear only within:

- a function;
- a method;
- a lambda

whose declared or inferred return type supports the relevant propagation.

There is no `try` prefix.

This syntax is invalid:

```zen
try loadUser()?
```

The canonical syntax is:

```zen
loadUser()?
```

---

# 72. Async functions

An async function declares the type produced when its work completes.

```zen
async fn loadUser(id: Int)
    -> Result<User, NetworkError> {
    ...
}
```

The declared type is:

```text
Result<User, NetworkError>
```

but invoking the function produces:

```text
Task<Result<User, NetworkError>>
```

Conceptually:

```text
call(async fn (...) -> T) : Task<T>
```

`Task<T>` is a core runtime abstraction exposed through the standard library.

---

# 73. Await

If:

```text
Γ ⊢ e : Task<T>
```

then:

```zen
await e
```

has type:

```text
T
```

`await` may only appear inside an asynchronous function or asynchronous lambda.

There is no implicit awaiting.

---

# 74. Async error propagation

Because `await` and `?` are separate operations:

```zen
let user = (await loadUser(id))?;
```

is always semantically valid when the types permit it.

The canonical formatter may allow:

```zen
let user = await loadUser(id)?;
```

with parsing defined as:

```zen
(await loadUser(id))?
```

`await` never implicitly catches or transforms a `Result`.

---

# 75. Structured tasks

The Zen runtime uses structured concurrency for standard async APIs.

A task spawned as a child of a scope remains logically owned by that scope unless an API explicitly transfers ownership.

The core language does not expose raw threads or shared-memory synchronization primitives in Zen 1.

Higher-level task APIs are defined by the standard library.

---

# 76. Defer

A `defer` statement schedules cleanup for the current lexical scope.

```zen
let file = open(path)?;
defer file.close();
```

Deferred operations execute:

- when the scope exits normally;
- before an explicit `return`;
- before propagation using `?`;
- before `break` or `continue` leaves the relevant scope.

Multiple deferred actions execute in reverse declaration order.

A deferred expression must return `Unit`.

---

# 77. For loops

```zen
for item in items {
    process(item);
}
```

The iteration protocol is defined through a compiler-known standard interface.

The loop variable is immutable.

A `for` loop has type `Unit`.

Zen 1 does not have C-style `for` loops.

---

# 78. While loops

```zen
while condition {
    process();
}
```

The condition must have type `Bool`.

A `while` loop has type `Unit`.

Infinite loops are permitted.

Nontermination is not a type error.

---

# 79. Break and continue

`break` and `continue` may only appear inside loops.

Zen 1 does not support value-producing `break`.

Therefore:

```zen
break value;
```

is invalid.

---

# 80. Collections

The standard prelude defines:

```text
List<T>
Set<T>
Map<K, V>
```

These are immutable value-oriented collections.

Mutation-like operations return new values.

```zen
let updated = users.append(user);
```

A mutable local may be rebound:

```zen
var users = List<User>.empty();
users = users.append(user);
```

---

# 81. Collection access

Dynamic collection indexing using `[]` does not exist in Zen 1.

Use:

```zen
users.get(index)
```

which returns:

```text
Option<User>
```

A missing value therefore cannot create a bounds exception.

---

# 82. Maps

Map lookup returns `Option`.

```zen
let user = usersById.get(id);
```

Map keys must satisfy the appropriate standard-library equality and hashing constraints.

A missing key is normal absence, not a runtime failure.

---

# 83. List literals

```zen
let numbers = [1, 2, 3];
```

All elements must have one compatible static type.

An empty literal:

```zen
[]
```

requires a known expected type.

For example:

```zen
let users: List<User> = [];
```

is valid.

Without contextual type information, an empty list literal is a compile-time error.

---

# 84. Operators

Zen's operators are compiler-defined.

Users cannot define new operators or overload existing operators.

Core operators include:

```text
+
-
*
<
<=
>
>=
==
!=
&&
||
!
```

The exact accepted operand types are statically defined by the language and standard prelude.

---

# 85. Boolean operators

```text
&&
||
!
```

operate only on `Bool`.

`&&` and `||` use short-circuit evaluation.

---

# 86. Equality

`==` and `!=` are available only for equality-capable types.

Primitive examples include:

```text
Bool
Int
Float
String
Char
fixed-width numeric types
```

Structs and enums are equality-capable when all contained fields or payload values are equality-capable.

Function values are not equality-capable.

Native opaque handles are not equality-capable unless their binding explicitly defines safe identity semantics.

---

# 87. Structural equality

For equality-capable structs, equality recursively compares fields.

For equality-capable enums:

1. variants must match;
2. corresponding payloads must compare equal.

There is no user-defined `==` operator overload.

---

# 88. Comparison

Ordering operators:

```text
<
<=
>
>=
```

require types with language-defined ordering semantics.

Operands must have compatible static types.

Zen does not implicitly compare:

```text
Int
```

against:

```text
Float
```

---

# 89. Logical expression precedence

From highest to lowest, the initial precedence order is:

```text
postfix:
    call
    member access
    ?

unary:
    !
    unary -
    await

multiplicative:
    *

additive:
    +
    -

comparison:
    <
    <=
    >
    >=

equality:
    ==
    !=

logical AND:
    &&

logical OR:
    ||
```

Parentheses may always be used to make precedence explicit.

The formatter should introduce parentheses when doing so materially improves readability.

---

# 90. Core expression grammar

The following EBNF is intentionally simplified around semantic categories.

```ebnf
expression =
    if_expression
  | match_expression
  | lambda_expression
  | logical_or_expression;

logical_or_expression =
    logical_and_expression,
    { "||", logical_and_expression };

logical_and_expression =
    equality_expression,
    { "&&", equality_expression };

equality_expression =
    comparison_expression,
    { ( "==" | "!=" ), comparison_expression };

comparison_expression =
    additive_expression,
    [ ( "<" | "<=" | ">" | ">=" ), additive_expression ];

additive_expression =
    multiplicative_expression,
    { ( "+" | "-" ), multiplicative_expression };

multiplicative_expression =
    unary_expression,
    { "*", unary_expression };

unary_expression =
    ( "!" | "-" | "await" ), unary_expression
  | postfix_expression;

postfix_expression =
    primary_expression,
    {
        call_suffix
      | member_suffix
      | "?"
    };

member_suffix =
    ".",
    identifier;

call_suffix =
    "(",
    [ call_arguments ],
    ")";

call_arguments =
    positional_arguments
  | named_arguments;

positional_arguments =
    expression,
    { ",", expression },
    [ "," ];

named_arguments =
    named_argument,
    { ",", named_argument },
    [ "," ];

named_argument =
    identifier,
    ":",
    expression;
```

---

# 91. Primary expressions

```ebnf
primary_expression =
    literal
  | identifier
  | "self"
  | "(" expression ")"
  | block
  | list_literal
  | struct_literal
  | contextual_enum_constructor;
```

Qualified identifiers and generic arguments are resolved during parsing and name resolution according to their syntactic context.

---

# 92. If grammar

```ebnf
if_expression =
    "if",
    expression,
    block,
    [ "else", ( block | if_expression ) ];
```

When used as a value, the type checker requires the `else` branch.

---

# 93. Match grammar

```ebnf
match_expression =
    "match",
    expression,
    "{",
    match_arm,
    { match_arm },
    "}";

match_arm =
    pattern,
    "=>",
    expression,
    ";";
```

A block may be used as the arm expression.

```zen
match result {
    .ok(value) => {
        log(value);
        value
    };

    .err(error) => fallback(error);
}
```

---

# 94. Block grammar

```ebnf
block =
    "{",
    { statement },
    [ expression ],
    "}";
```

The final expression must not end in a semicolon when it supplies the block's value.

---

# 95. Statement grammar

```ebnf
statement =
    let_statement
  | var_statement
  | assignment_statement
  | return_statement
  | defer_statement
  | while_statement
  | for_statement
  | break_statement
  | continue_statement
  | expression_statement;

let_statement =
    "let",
    identifier,
    [ ":", type ],
    "=",
    expression,
    ";";

var_statement =
    "var",
    identifier,
    [ ":", type ],
    "=",
    expression,
    ";";

assignment_statement =
    identifier,
    "=",
    expression,
    ";";

return_statement =
    "return",
    [ expression ],
    ";";

defer_statement =
    "defer",
    expression,
    ";";

break_statement =
    "break",
    ";";

continue_statement =
    "continue",
    ";";

expression_statement =
    expression,
    ";";
```

Assignment to arbitrary member expressions is deliberately absent.

---

# 96. While grammar

```ebnf
while_statement =
    "while",
    expression,
    block;
```

---

# 97. For grammar

```ebnf
for_statement =
    "for",
    identifier,
    "in",
    expression,
    block;
```

---

# 98. Struct literal grammar

```ebnf
struct_literal =
    named_type,
    "{",
    [ struct_initializer,
      { ",", struct_initializer },
      [ "," ]
    ],
    "}";

struct_initializer =
    identifier,
    ":",
    expression;
```

Zen 1 does not provide implicit field shorthand such as:

```zen
User {
    name,
}
```

The explicit form is required:

```zen
User {
    name: name,
}
```

This is intentionally more verbose and less ambiguous for generated code.

---

# 99. Struct update grammar

Conceptually:

```ebnf
struct_update =
    expression,
    "with",
    "{",
    struct_initializer,
    { ",", struct_initializer },
    [ "," ],
    "}";
```

`with` is reserved by this construct even if not otherwise treated as a general-purpose keyword.

A compiler implementation should reserve it lexically from Zen 1 to prevent future ambiguity.

---

# 100. Static name resolution

Names are resolved using lexical scope.

For an unqualified value identifier, the compiler searches:

1. current local bindings;
2. parameters;
3. declarations in the current module;
4. explicit imports;
5. standard prelude declarations.

Ambiguity at a given level is a compile-time error.

The compiler never selects one candidate based on import order.

---

# 101. Type name resolution

Type names resolve independently from local value bindings.

A type declaration and local value may technically share a textual identifier because they exist in separate namespaces.

The canonical linter should discourage unnecessary same-name declarations when they reduce readability.

---

# 102. Public API completeness

Every declaration reachable through a public API must have a stable, fully determined static type.

A public declaration may not expose:

- unresolved inference variables;
- private types;
- anonymous compiler-generated types;
- platform-specific native implementation types unless explicitly part of that API.

This property allows complete API extraction without analyzing implementation bodies.

---

# 103. Exhaustive static checking

A Zen compiler must reject a program when it cannot prove a required safety property.

It may not replace missing proof with a runtime type check unless the language construct explicitly represents that check through `Option`, `Result`, or another typed abstraction.

For example, an invalid cast is not accepted merely because the runtime could detect it.

The source must express the possibility of failure.

---

# 104. Native safety boundary

The compiler assumes that a native binding obeys its declared Zen signature.

If native code declares:

```zen
native fn deviceName() -> String;
```

the native implementation is responsible for producing a valid Zen `String`.

Violating that contract is a native implementation defect.

Zen code receiving the value may rely on the declared type.

This trust boundary is necessary for foreign-function interoperability.

---

# 105. Platform-independent semantics

The following must not change by target:

- integer semantics;
- string semantics;
- enum layout as observed by Zen code;
- pattern-matching behavior;
- equality semantics;
- evaluation order;
- error-propagation behavior;
- async source semantics;
- visibility;
- generic type checking.

Implementation representation may vary.

Observable Zen behavior may not.

---

# 106. Evaluation order

Zen evaluates expressions from left to right unless a construct explicitly defines otherwise.

For:

```zen
f(a(), b(), c())
```

evaluation occurs as:

```text
a()
b()
c()
f(...)
```

For:

```zen
left() + right()
```

`left()` evaluates before `right()`.

This rule avoids platform-dependent evaluation order.

---

# 107. Short-circuit evaluation

For:

```zen
a && b
```

`b` is evaluated only if `a` is `true`.

For:

```zen
a || b
```

`b` is evaluated only if `a` is `false`.

---

# 108. No implicit dynamic dispatch

Generic constraints and interface implementations do not imply hidden existential boxing.

When generic code calls an interface method, the compiler may statically specialize or otherwise implement dispatch as long as observable semantics remain identical.

Zen 1 does not define first-class existential interface values.

A later specification may introduce an explicit form if cross-platform application APIs require one.

---

# 109. No general reflection

Core Zen does not allow arbitrary runtime enumeration of:

- fields;
- methods;
- type definitions;
- private members.

Serialization, UI binding, and native bridging should use compiler-defined metadata facilities with known static semantics.

---

# 110. Compiler-known annotations

The grammar may support annotations used by the compiler and platform.

Example:

```zen
@serializable
struct User {
    public id: Int;
}
```

Annotations do not execute arbitrary compile-time programs.

Every recognized annotation has specification-defined behavior.

Unknown annotations are compile-time errors.

A general macro system is outside Zen 1.

---

# 111. Formatter requirements

Zen defines one canonical formatter.

Formatting must be deterministic.

For valid source `s`:

```text
format(format(s)) = format(s)
```

The formatter may change whitespace and line wrapping.

It may not change semantic structure.

Machine-generated code should normally be formatted before storage or presentation.

---

# 112. Diagnostics

Compiler diagnostics must expose stable machine-readable identifiers.

Conceptually:

```text
code
severity
message
source_range
expected_type
actual_type
related_symbols
suggested_repairs
```

A language model should be able to respond to a diagnostic without parsing compiler prose heuristically.

Example:

```text
ZEN-TYPE-0041

Cannot propagate Result<User, NetworkError>
from a function returning Result<Profile, AppError>.

No implicit error conversion exists.

Suggested repair:
Map NetworkError to AppError before applying `?`.
```

---

# 113. Deterministic repair

When possible, diagnostics should distinguish between:

- syntax errors;
- unresolved names;
- incorrect argument count;
- incorrect argument label;
- type mismatch;
- generic inference failure;
- missing enum cases;
- ambiguous method resolution;
- invalid native boundary types.

Diagnostics should not collapse unrelated problems into a generic type error.

This is especially important for AI-assisted repair loops.

---

# 114. Core grammar overview

At the top level:

```ebnf
source_file =
    { import_declaration },
    { declaration };

declaration =
    struct_declaration
  | enum_declaration
  | interface_declaration
  | implementation_declaration
  | function_declaration
  | native_function_declaration
  | const_declaration;
```

Declarations may additionally carry compiler-defined annotations.

---

# 115. Constructs intentionally absent

Zen 1 intentionally has no syntax for:

```text
class inheritance
throw
catch
try
null
pointer dereference
pointer arithmetic
goto
operator declarations
user macros
implicit constructors
destructors
property setters
mutable global state
dynamic field creation
method overloading
function overloading
wildcard imports
unchecked cast
```

Their absence is part of the language design rather than missing implementation work.

---

# 116. Relationship with the Zen platform

The following are not defined by this document:

```text
component
state
view
UI element syntax
navigation
effects
HTTP
WebSockets
location
permissions
storage
database APIs
camera
notifications
animations
platform capabilities
application lifecycle
```

These features will be defined by the Zen Platform Specification.

They must obey all safety guarantees in this document.

The platform may introduce limited compiler-recognized syntax where doing so materially improves correctness, but it may not weaken the core type system.

---

# 117. UI syntax boundary

JSX-like Zen UI syntax is not a general core-language expression in Draft 0.1.

It is valid only inside platform-defined UI contexts such as:

```zen
view {
    ...
}
```

This allows the UI grammar to evolve without contaminating ordinary expression parsing.

Values embedded into UI syntax remain ordinary statically typed Zen expressions.

---

# 118. Soundness objective

Ignoring explicit native trust boundaries and process-level resource exhaustion, Zen's central soundness property is:

If:

```text
Γ ⊢ e : T
```

and evaluation of `e` completes normally, then the resulting value is a valid inhabitant of `T`.

Evaluation cannot spontaneously produce a value outside `T`.

Operations whose failure is expected must encode that failure in their static result type.

---

# 119. Runtime-error objective

Safe Zen should never require a programmer to catch a language-generated runtime exception.

Consequently, APIs such as these are invalid designs:

```text
List.getUnsafe(index) -> T
String.toInt() -> Int // throws on malformed input
Map.require(key) -> V // throws when absent
cast<T>(value) -> T // throws when incorrect
```

The safe designs are:

```text
List.get(index) -> Option<T>

String.parseInt()
    -> Result<Int, ParseError>

Map.get(key)
    -> Option<V>

value.as<T>()
    -> Option<T>
```

The type signature must communicate the possibility of failure.

---

# 120. Canonicality objective

Zen deliberately optimizes for a low number of semantically equivalent spellings.

The compiler and standard library should resist adding aliases such as:

```text
map / transform / select
flatMap / bind / andThen
interface / trait / protocol
none / nil / null
```

unless the concepts are genuinely distinct.

A model generating Zen should have as few equally plausible syntax choices as practical.

---

# 121. LLM generation objective

A Zen code generator should normally be able to construct correct code using:

1. visible local declarations;
2. imported API signatures;
3. compiler diagnostics;
4. the static type of each expression.

Correct generation should not require knowledge of:

- global overload ranking;
- undocumented coercions;
- hidden exceptions;
- runtime monkey patching;
- extension-method import order;
- macro expansion semantics;
- contextual null behavior.

This property is a core language requirement rather than merely a tooling goal.

---

# 122. Resolutions from Language Specification Draft 0.1

This document resolves several previously underspecified points.

`try` is not part of Zen syntax. Error propagation uses only:

```zen
operation()?
```

Async functions declare their completed value type. Calling one produces `Task<T>`, and `await` extracts `T`.

`?` performs no implicit error conversion.

Struct fields remain immutable even when the local variable containing the struct uses `var`.

Local variable shadowing is prohibited.

Methods cannot be arbitrarily injected into types from unrelated modules.

Generic types are invariant.

Zen 1 has only minimal subtyping, with `Never` as the universal subtype.

Dynamic collection indexing is absent.

Fixed-width integer arithmetic that can overflow uses explicit checked operations.

Named and positional arguments may not be mixed in one call.

Public APIs always require explicit types.

Interface conformance does not create a class-like inheritance hierarchy.

---

# 123. Compiler implementation order

A practical first compiler should be able to implement the core language in roughly this semantic order:

```text
lexer
parser
AST
module/import resolver
symbol table
nominal type representation
local type checking
structs
enums
match exhaustiveness
functions
generics
interfaces
method resolution
Option / Result
? propagation
async / Task
native declarations
diagnostics
canonical formatter
```

The component and UI system should be built only after this core is stable enough that the platform can rely on it rather than introducing special-case unsoundness.

---

# 124. Final design rule

Whenever Zen has to choose between:

```text
shorter source code
```

and:

```text
more statically obvious source code
```

Zen should normally choose the latter.

The intended optimization target is not minimum keystrokes.

It is:

> **maximum probability that generated code has exactly one intended meaning and that incorrect code is rejected before it runs.**