# Zen Language Specification

**Status:** Draft 0.1  
**Purpose:** Initial compiler and standard-library specification  
**Primary targets:** Web, iOS, Android  
**Design priority:** Correctness → Predictability → Simplicity → Performance → Convenience

---

# 1. Overview

Zen is a statically typed programming language and application platform for building cross-platform software.

Zen is designed primarily for code that is generated, modified, reviewed, and maintained with the assistance of large language models.

Its design therefore favors:

- one obvious way to express common operations;
- explicit semantics over implicit behavior;
- small language surface area;
- strong static guarantees;
- predictable syntax;
- minimal context-dependent behavior;
- deterministic compilation;
- simple code transformation;
- straightforward machine-readable diagnostics.

Zen programs should fail to compile rather than fail unexpectedly at runtime.

A valid program written entirely in safe Zen should not produce a runtime language error.

Failures originating outside the Zen language — operating-system APIs, network operations, native libraries, malformed external data, unavailable resources, and similar conditions — are represented explicitly as values.

---

# 2. Goals

Zen has six primary goals.

## 2.1 Soundness

The static type system must be sound.

If the compiler determines that an expression has type `T`, evaluating the expression must either:

1. produce a valid `T`; or
2. produce an explicitly represented failure already present in the expression's type.

There is no undefined behavior in safe Zen.

---

## 2.2 No implicit runtime failure

The following must not result in hidden runtime exceptions:

- null access;
- invalid casts;
- array bounds violations;
- integer overflow;
- integer division by zero;
- uninitialized values;
- missing enum cases;
- failed optional access;
- use-after-free;
- data races;
- failed implicit conversions.

Operations capable of failing must expose that possibility in their types.

---

## 2.3 AI-first syntax

Zen is optimized for machine generation.

This means Zen intentionally avoids features that frequently produce ambiguous or subtly incorrect generated code.

Zen should prefer:

```zen
let user: User = ...;
```

over constructs whose meaning depends heavily on inference, overload resolution, macros, or surrounding context.

Public APIs should be understandable primarily from their signatures.

---

## 2.4 Cross-platform semantics

A Zen program should behave the same way on:

- Web;
- iOS;
- Android.

Platform differences belong in libraries and native boundaries rather than in the semantics of the language.

---

## 2.5 Native interoperability

Zen must be capable of calling:

- JavaScript and Web APIs;
- Swift and Objective-C;
- Kotlin and Java;
- C-compatible native libraries where supported.

Native code must also be able to invoke exported Zen functions.

Native interoperability is an explicit boundary outside the guarantees provided by safe Zen.

---

## 2.6 Small language, large platform

The language should remain small.

Functionality such as:

- networking;
- HTTP;
- location;
- storage;
- databases;
- UI;
- animations;
- camera access;
- notifications;
- sensors;
- cryptography;

belongs primarily to the Zen platform libraries.

The language provides the safety mechanisms needed to implement those APIs.

---

# 3. Non-goals

Zen does not attempt to provide:

- C-level manual memory management;
- arbitrary pointer arithmetic;
- metaprogramming through unrestricted macros;
- multiple inheritance;
- operator overloading;
- implicit numeric coercion;
- implicit nullability;
- exception-based control flow;
- runtime reflection as a fundamental programming model;
- unrestricted shared mutable state;
- highly sophisticated type-level programming.

Zen is not intended to maximize expressiveness.

Zen is intended to maximize the probability that a program saying something simple actually means something simple.

---

# 4. Source files

Zen source files use:

```text
.zen
```

A file implicitly represents a module based on its package-relative path.

For example:

```text
src/models/user.zen
```

defines:

```text
models.user
```

No module declaration is required.

---

# 5. Lexical syntax

Zen source code uses UTF-8.

Identifiers are case-sensitive.

```zen
user
User
currentUser
```

are distinct identifiers.

## Comments

```zen
// Single-line comment

/*
Multi-line comment
*/
```

Comments do not affect program semantics.

---

# 6. Statements and formatting

Zen uses braces for blocks.

Statements terminate with `;`.

```zen
let name = "Ahmed";
let age = 33;
```

The formatter may place expressions across multiple lines without changing semantics.

There is no automatic semicolon insertion.

A canonical formatter is part of the Zen toolchain.

Generated Zen code is expected to be formatter-stable:

```text
format(format(source)) == format(source)
```

---

# 7. Variables

Values are immutable by default.

```zen
let name = "Zen";
```

A mutable local binding uses `var`.

```zen
var count = 0;
count = count + 1;
```

Mutation must always be explicit.

Function parameters are immutable.

```zen
fn increment(value: Int) -> Int {
    return value + 1;
}
```

Assignments to undeclared variables are compile-time errors.

---

# 8. Primitive types

Zen defines the following fundamental types:

```text
Bool
Int
Float
String
Char
Unit
Never
```

Additional fixed-width numeric types exist primarily for native interoperability:

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

---

# 9. Integer semantics

`Int` represents an arbitrary-precision signed integer.

```zen
let value: Int = 10;
```

`Int` does not overflow.

This avoids platform differences between JavaScript, ARM, and other native targets.

Performance-sensitive or native-facing code may explicitly use fixed-width numeric types.

Fixed-width arithmetic is checked.

Operations that exceed their valid range return a `Result`.

For example:

```zen
let result: Result<I32, ArithmeticError> =
    I32.checkedAdd(a, b);
```

There is no implicit wrapping arithmetic.

Explicit wrapping functionality may exist in the standard library:

```zen
I32.wrappingAdd(a, b)
```

---

# 10. Floating-point semantics

`Float` is an IEEE-754 64-bit floating-point value.

It is equivalent to `F64`.

Floating-point values may contain:

- infinity;
- negative infinity;
- NaN.

These are values rather than runtime failures.

---

# 11. Numeric conversion

Zen performs no implicit numeric narrowing or widening.

Invalid conversions must be represented explicitly.

```zen
let count: Result<I32, ConversionError> =
    I32.from(value);
```

Conversions known to be lossless may return their target value directly.

```zen
let value: Int = Int.from(smallValue);
```

Native API boundaries must perform explicit numeric conversion.

---

# 12. Strings

`String` is immutable Unicode text.

Zen does not permit integer indexing directly into strings.

The following is invalid:

```zen
text[4]
```

because the meaning of a character index is ambiguous for Unicode text.

String APIs explicitly specify their unit.

Examples may include:

```zen
text.characters()
text.graphemes()
text.codePoints()
text.utf8()
```

---

# 13. Unit

`Unit` represents the absence of a meaningful result.

```zen
fn log(message: String) -> Unit {
    Console.write(message);
}
```

Functions returning `Unit` may omit the explicit return statement.

---

# 14. Never

`Never` is the type of an expression that cannot return normally.

Examples include:

```zen
process.exit(1)
```

or compiler-proven unreachable branches.

`Never` is compatible with any expected result type.

---

# 15. Product types

Structured values are declared using `struct`.

```zen
struct User {
    id: UserId;
    name: String;
    age: Int;
}
```

Values are created explicitly:

```zen
let user = User {
    id: userId,
    name: "Ahmed",
    age: 33,
};
```

Fields must all be initialized unless they have declared defaults.

There are no uninitialized fields.

---

# 16. Value semantics

Structs have value semantics by default.

```zen
let first = user;
let second = first;
```

Conceptually, `first` and `second` represent values rather than mutable object identities.

Implementations may share immutable backing storage internally.

This optimization must not be observable.

---

# 17. Updating immutable values

Immutable structures may be copied with modifications.

```zen
let olderUser = user with {
    age: user.age + 1,
};
```

The original value remains unchanged.

---

# 18. Sum types

Zen uses `enum` for tagged unions.

```zen
enum LoadingState<T> {
    idle;
    loading;
    loaded(T);
    failed(AppError);
}
```

Enum variants may contain values.

```zen
let state = LoadingState<User>.loaded(user);
```

Enums are closed.

All possible variants are known at compile time.

---

# 19. Pattern matching

`match` performs exhaustive pattern matching.

```zen
match state {
    .idle => showIdle();
    .loading => showSpinner();

    .loaded(user) => {
        showUser(user);
    }

    .failed(error) => {
        showError(error);
    }
}
```

Omitting a variant is a compile-time error.

A wildcard pattern may be used explicitly:

```zen
match state {
    .loaded(user) => use(user);
    _ => ignore();
}
```

Using `_` means the programmer intentionally accepts future or otherwise unmatched possibilities.

---

# 20. Option

Zen has no `null`.

Potential absence is represented with:

```zen
Option<T>
```

with the variants:

```zen
enum Option<T> {
    some(T);
    none;
}
```

Example:

```zen
let user: Option<User> = findUser(id);
```

Values can be inspected with `match`:

```zen
match user {
    .some(value) => show(value);
    .none => showMissingUser();
}
```

Optional chaining may be supported as syntax sugar where its behavior is unambiguous:

```zen
let name: Option<String> = user?.name;
```

It always produces an `Option`.

---

# 21. Result

Recoverable failure is represented using:

```zen
Result<T, E>
```

Conceptually:

```zen
enum Result<T, E> {
    ok(T);
    err(E);
}
```

Example:

```zen
fn loadUser(id: UserId) -> Result<User, NetworkError> {
    ...
}
```

Errors are values.

Zen has no general exception mechanism.

---

# 22. Error propagation

`?` propagates `Result` or `Option`.

```zen
fn loadProfile(id: UserId) -> Result<Profile, AppError> {
    let user = try loadUser(id)?;
    let avatar = try loadAvatar(user.avatarId)?;
    return .ok(Profile(user, avatar));
}
```

The exact accepted shorthand should eventually become:

```zen
let user = loadUser(id)?;
```

The compiler verifies that the enclosing return type can represent the propagated failure.

`?` cannot silently discard errors.

---

# 23. Functions

Functions use `fn`.

```zen
fn greet(name: String) -> String {
    return "Hello, " + name;
}
```

All function parameters require type annotations.

All named functions require explicit return types.

This is intentional.

Zen prefers stable API signatures over aggressive function-level type inference.

---

# 24. Function values

Functions are first-class values.

```zen
let transform: (Int) -> Int = increment;
```

Function types use:

```text
(A, B) -> C
```

Example:

```zen
fn mapValue(
    value: Int,
    transform: (Int) -> String
) -> String {
    return transform(value);
}
```

---

# 25. Lambdas

Anonymous functions use a canonical syntax.

```zen
let double = (value: Int) -> Int {
    return value * 2;
};
```

When parameter and return types are known from the surrounding context, local annotations may be omitted.

```zen
numbers.map((value) {
    return value * 2;
});
```

Inference is contextual and local only.

---

# 26. No function overloading

Two functions in the same namespace may not differ only by parameter types.

This is invalid:

```zen
fn parse(value: String) -> Int;
fn parse(value: String) -> Float;
```

Use explicit names:

```zen
parseInt(value)
parseFloat(value)
```

This makes generated code, documentation, and tool resolution considerably more deterministic.

---

# 27. Default arguments

Default arguments are permitted when compile-time deterministic.

```zen
fn request(
    url: Url,
    timeout: Duration = Duration.seconds(30)
) -> Result<Response, HttpError> {
    ...
}
```

A caller may use:

```zen
request(url);
```

or:

```zen
request(
    url: url,
    timeout: Duration.seconds(10),
);
```

Named arguments are recommended when a function has multiple parameters of similar types.

---

# 28. Control flow

Zen provides:

```text
if
else
match
for
while
return
break
continue
```

There is no C-style `for (;;)` syntax.

---

# 29. If

```zen
if authenticated {
    showApplication();
} else {
    showLogin();
}
```

Conditions must have type `Bool`.

Zen does not define truthiness.

The following is invalid:

```zen
if users {
}
```

---

# 30. If expressions

`if` may produce a value.

```zen
let title = if authenticated {
    "Home"
} else {
    "Sign in"
};
```

Both branches must produce compatible types.

---

# 31. Loops

Collection iteration uses `for`.

```zen
for user in users {
    render(user);
}
```

Iteration protocols are defined by the standard library.

A conditional loop uses:

```zen
while condition {
    ...
}
```

---

# 32. Collections

Core standard collections include:

```text
List<T>
Map<K, V>
Set<T>
```

Collections are immutable values by default.

```zen
let names = ["Ali", "Mona"];
let updated = names.append("Sara");
```

Mutable collection builders may be used locally for performance.

Their mutability cannot escape their declared scope without becoming an explicit mutable reference type.

---

# 33. Safe indexing

Dynamic collections do not provide an unchecked indexing operator.

Instead:

```zen
let value: Option<User> = users.get(index);
```

This is intentionally invalid:

```zen
let value = users[index];
```

unless the compiler can statically prove that the accessed structure and index are valid, such as certain fixed-size tuple operations.

There is no array-out-of-bounds runtime exception in safe Zen.

---

# 34. Maps

Map lookup returns `Option`.

```zen
let user: Option<User> = usersById.get(id);
```

Accessing a missing key cannot raise an exception.

---

# 35. Equality

Value types use structural equality.

```zen
first == second
```

compares their values.

There is no user-defined operator overload for `==`.

Types containing values that cannot meaningfully be compared do not automatically support equality.

---

# 36. Identity

Reference identity must be explicit.

Zen may expose an identity-bearing type such as:

```text
Ref<T>
```

for APIs that require it.

Normal application data should use values rather than object identity.

---

# 37. Interfaces

Shared behavior may be described using `interface`.

```zen
interface Serializable {
    fn serialize(self) -> String;
}
```

A type declares implementation explicitly:

```zen
impl Serializable for User {
    fn serialize(self) -> String {
        ...
    }
}
```

Zen interfaces intentionally avoid several advanced mechanisms in version 1:

- no associated types;
- no specialization;
- no overlapping implementations;
- no implicit implementation;
- no higher-kinded types.

The goal is predictable resolution.

---

# 38. Generics

Types and functions may have generic parameters.

```zen
struct Pair<A, B> {
    first: A;
    second: B;
}
```

```zen
fn first<T>(items: List<T>) -> Option<T> {
    return items.first();
}
```

Constraints are explicit.

```zen
fn serialize<T: Serializable>(value: T) -> String {
    return value.serialize();
}
```

Generics are invariant unless a later specification explicitly introduces safe variance.

---

# 39. Type inference

Zen supports local type inference.

```zen
let name = "Zen";
let count = 10;
```

The compiler infers:

```text
String
Int
```

Inference must not alter public APIs.

Public declarations require explicit externally observable types.

Zen does not perform global inference.

---

# 40. No implicit `Any`

Zen has no implicit dynamically typed escape hatch.

A value never silently becomes `Any`.

A general runtime value container may exist explicitly:

```text
Dynamic
```

or:

```text
Value
```

but accessing a concrete type requires checked conversion.

For example:

```zen
let name: Option<String> = value.as<String>();
```

An invalid dynamic cast cannot crash a safe Zen program.

---

# 41. Casting

Unchecked casts do not exist in safe Zen.

A potentially invalid cast returns an `Option` or `Result`.

```zen
let dog: Option<Dog> = animal.as<Dog>();
```

Conversions that are statically guaranteed may occur directly.

---

# 42. Modules

Imports use absolute package paths.

```zen
import app.models.User;
import zen.http.HttpClient;
```

Multiple equivalent import syntaxes should not be introduced.

Wildcard imports are not supported.

This avoids symbol ambiguity and improves generated-code reliability.

---

# 43. Visibility

Declarations are private to their module unless marked `public`.

```zen
public struct User {
    public id: UserId;
    public name: String;
}
```

Library APIs therefore expose their intended surface explicitly.

This also makes API extraction straightforward for language models and tooling.

---

# 44. Memory safety

Safe Zen provides:

- no raw pointers;
- no manual memory deallocation;
- no dangling references;
- no use-after-free;
- no uninitialized memory.

Objects managed by Zen use implementation-defined automatic memory management.

Native implementations may use tracing garbage collection, reference counting, region allocation, escape analysis, or other strategies.

These strategies must not alter observable Zen semantics.

Destructors are not part of ordinary Zen object semantics.

Resource cleanup should be explicit.

---

# 45. Resources

Resources such as:

- files;
- database handles;
- native objects;
- sockets;

must expose explicit lifecycle APIs.

Scoped cleanup may use `defer`.

```zen
let file = openFile(path)?;
defer file.close();

useFile(file);
```

`defer` executes when control leaves the current scope, including through `return` or `?`.

Because Zen has no language exceptions, its behavior remains deterministic.

---

# 46. Async functions

Asynchronous functions use `async`.

```zen
async fn loadUser(id: UserId) -> Result<User, NetworkError> {
    let response = await http.get("/users/" + id);
    return decodeUser(response.body);
}
```

`await` may only appear within an async context.

Async failures are still represented using `Result`.

There is no special asynchronous exception mechanism.

---

# 47. Structured concurrency

Zen uses structured concurrency.

Child tasks belong to an explicit lifetime.

A task may not silently continue forever after its owning scope has been destroyed unless deliberately detached through an explicit platform API.

The application framework automatically cancels UI-owned work when its owner is unmounted or destroyed.

Cancellation is a normal state rather than an exception.

---

# 48. Shared state and concurrency

Zen does not expose unrestricted shared mutable memory.

Normal values are immutable and safely shareable.

Asynchronous application code should communicate through:

- immutable values;
- message passing;
- framework-managed state;
- actors or workers where true parallelism is required.

The initial Zen concurrency model does not include user-visible locks, mutexes, or atomic memory operations.

Low-level concurrency belongs behind native or specialized library boundaries.

This prevents data races in ordinary Zen code.

---

# 49. Native code boundary

Native interoperability is an explicit trust boundary.

A native declaration uses `native`.

Conceptually:

```zen
native fn getCurrentLocation()
    -> Result<Location, NativeError>;
```

Native bindings may violate assumptions that Zen itself guarantees.

The compiler therefore distinguishes:

```text
safe Zen
native declarations
unsafe implementation code
```

Errors entering Zen from native systems must normally be translated into typed values.

---

# 50. Native errors

The platform provides a common native error representation.

```zen
struct NativeError {
    domain: String;
    code: String;
    message: String;
}
```

Platform libraries should normally map native errors into more meaningful typed Zen errors.

For example:

```zen
enum LocationError {
    permissionDenied;
    unavailable;
    timedOut;
    native(NativeError);
}
```

Application code should rarely need to inspect raw native failures.

---

# 51. Platform-specific native implementations

A shared Zen declaration may have platform-specific native implementations.

For example:

```zen
public fn requestLocation()
    -> Result<Location, LocationError>;
```

may map internally to:

- Core Location on iOS;
- Android Location APIs;
- Geolocation API on the web.

The calling Zen program does not change.

---

# 52. Serialization

Serialization is explicit and type-directed.

Zen should eventually support compiler-derived serializers for formats such as JSON.

Example:

```zen
@serializable
struct User {
    id: String;
    name: String;
}
```

However, annotations must not act as unrestricted procedural macros.

Compiler-supported annotations have clearly specified semantics.

Unknown annotations are compile-time errors.

---

# 53. JSON

JSON decoding must be safe.

Given:

```zen
struct User {
    id: String;
    age: Int;
}
```

decoding external JSON produces:

```zen
Result<User, DecodeError>
```

not `User`.

Malformed or structurally invalid external data cannot create an invalid Zen value.

---

# 54. Compile-time guarantees

For entirely safe Zen code, successful compilation guarantees at minimum:

- every variable is initialized before use;
- every value has a valid static type;
- no null dereferences;
- no invalid unchecked casts;
- no unhandled language exceptions;
- no collection bounds exceptions;
- no integer overflow for `Int`;
- no unchecked overflow for fixed-width integers;
- exhaustive enum handling where required;
- function calls have valid argument types;
- returned values match declared return types;
- immutable values cannot be mutated;
- private declarations cannot be accessed externally;
- generic constraints are satisfied;
- native-only operations cannot be mistaken for safe Zen operations.

---

# 55. Diagnostics

Compiler diagnostics are part of Zen's AI-first design.

Errors should be structured internally rather than existing only as prose.

A diagnostic should contain information equivalent to:

```text
code
severity
source range
summary
explanation
expected type
received type
related symbols
possible fixes
```

Example human output:

```text
ZEN-TYPE-0017

Expected:
    UserId

Received:
    String

at:
    src/profile.zen:18:21

No implicit conversion exists from String to UserId.

Possible fix:
    UserId.parse(value)
```

Stable diagnostic codes are required.

This allows automated repair systems to respond deterministically.

---

# 56. Canonical code

Zen intentionally avoids multiple equivalent constructs where possible.

For example, the language should not simultaneously provide:

```text
interface
protocol
trait
abstract class
```

for closely related purposes.

It chooses one concept.

Likewise, Zen should prefer one standard iteration mechanism, one standard error mechanism, and one standard optional-value mechanism.

The canonical formatter is expected to erase irrelevant stylistic differences.

These constraints are especially important for generated code.

---

# 57. Framework relationship

The Zen language does not itself contain concepts such as:

- buttons;
- HTTP requests;
- screens;
- navigation;
- location;
- application state.

These belong to the Zen platform.

However, the platform is designed together with the language and may use limited compiler-recognized constructs when they substantially improve safety.

---

# 58. Components

The planned Zen UI system introduces a first-class `component` declaration.

A preliminary form is:

```zen
component Counter {
    state count: Int = 0;

    fn increment() -> Unit {
        count.set(count + 1);
    }

    view {
        <Column>
            <Text>{count}</Text>

            <Button onPress={increment}>
                Increment
            </Button>
        </Column>
    }
}
```

Component state is not ordinary unrestricted mutable memory.

It is framework-managed reactive state.

Reading state returns its current value.

Modification must occur through explicit state operations.

---

# 59. Cross-platform UI

The UI platform should expose semantic components wherever possible.

For example:

```zen
<Button>
<Text>
<Image>
<TextInput>
<Scroll>
```

rather than requiring application code to choose between:

```text
HTML
UIKit
SwiftUI
Jetpack Compose
Android Views
```

Platform implementations translate Zen UI primitives into appropriate native or web behavior.

Escape hatches remain available through native interop.

---

# 60. Platform capabilities

Cross-platform services should follow the same model.

For example:

```zen
let permission =
    await Location.requestPermission();

match permission {
    .granted => {
        let location = await Location.current()?;
    }

    .denied => {
        showPermissionHelp();
    }

    .unavailable => {
        showUnsupportedMessage();
    }
}
```

Operating-system differences are represented through types rather than hidden runtime assumptions.

---

# 61. Capability availability

An API that cannot exist on every target must expose that fact.

Zen should prefer:

```zen
Capability<Feature>
```

or typed availability queries over runtime crashes.

A program should never discover through an exception that a platform feature does not exist.

---

# 62. Package manifest

A Zen application uses a declarative package manifest.

Proposed filename:

```text
zen.json
```

or eventually:

```text
zen.toml
```

The exact format is not yet normative.

The manifest describes:

- package name;
- Zen version;
- dependencies;
- supported targets;
- application capabilities;
- native dependencies;
- build configuration.

Source files should not contain build-system logic.

---

# 63. No general macros

Zen version 1 does not include a general macro system.

Macros significantly increase:

- syntax complexity;
- invisible control flow;
- compiler complexity;
- generated-code variability;
- difficulty of static analysis;
- difficulty of AI repair.

Repeated patterns should instead be solved through:

- functions;
- generics;
- components;
- compiler-defined annotations;
- build-time code generation with explicit generated files.

---

# 64. No inheritance

Zen has no class inheritance hierarchy.

Code reuse uses:

- composition;
- functions;
- interfaces;
- generic functions.

A `Dog` does not inherit implementation from `Animal`.

If both share behavior, that behavior is modeled explicitly.

---

# 65. No language exceptions

The following syntax does not exist:

```text
try
catch
throw
```

for ordinary Zen errors.

All expected failures use `Result`.

This means a function's failure behavior is visible in its signature.

```zen
fn parseUser(input: String)
    -> Result<User, ParseError>;
```

rather than:

```zen
fn parseUser(input: String) -> User;
// secretly throws
```

---

# 66. Panic and fatal termination

Safe libraries must not rely on panic for ordinary failures.

The platform may expose a fatal termination mechanism for conditions representing broken program invariants.

For example:

```zen
fatal("Compiler-generated state invariant violated");
```

`fatal` returns `Never`.

Its use should be uncommon and may generate compiler or linter warnings outside tests and generated framework internals.

It is not an error-handling mechanism.

---

# 67. Assertions

Assertions are primarily development and testing tools.

```zen
assert(user.age >= 0);
```

An assertion failing indicates a programmer invariant violation rather than recoverable application failure.

Assertions must never be used to validate untrusted external input.

Production behavior for assertions will be specified separately.

---

# 68. Testing

Tests are ordinary Zen functions marked with a compiler-recognized annotation.

```zen
@test
fn additionWorks() -> Unit {
    assertEqual(2 + 2, 4);
}
```

Async tests are supported:

```zen
@test
async fn loadsUser() -> Result<Unit, TestError> {
    let user = await loadUser(testId)?;
    assertEqual(user.name, "Ahmed");

    return .ok(unit);
}
```

---

# 69. Version 1 restrictions

The first version of Zen should deliberately exclude:

- user-defined operators;
- implicit conversions;
- implicit nullability;
- inheritance;
- exceptions;
- macros;
- variadic generics;
- higher-kinded types;
- dependent types;
- associated types;
- extension-resolution ambiguity;
- runtime monkey patching;
- prototype mutation;
- unrestricted reflection;
- pointer arithmetic;
- shared-memory concurrency;
- custom destructors.

Features may be introduced later only when a concrete application-development problem justifies their complexity.

---

# 70. Design rule for new features

A proposed language feature should normally be rejected if its main justification is:

> "This lets programmers write less code."

It should instead demonstrate at least one of:

- eliminating a class of bugs;
- making intent substantially clearer;
- enabling an important platform capability;
- improving generated-code correctness;
- making static analysis stronger;
- significantly improving performance without weakening semantics.

Zen accepts some verbosity when that verbosity makes code unambiguous.

---

# 71. Design rule for AI-generated code

Given two language designs with equivalent expressive power, Zen should prefer the design that minimizes the amount of hidden context an LLM must understand to generate correct code.

Therefore Zen favors:

```zen
let user: Result<User, NetworkError> = loadUser(id);
```

over behavior determined by:

- invisible implicit conversions;
- global extension methods;
- overload ranking;
- dynamic dispatch rules;
- macro expansion;
- hidden exception behavior;
- contextual nullability;
- runtime reflection.

A Zen source file should contain as much of its own meaning as reasonably practical.

---

# 72. Example program

```zen
import zen.app.App;
import zen.http.Http;
import zen.ui.Button;
import zen.ui.Column;
import zen.ui.Text;

struct User {
    id: Int;
    name: String;
}

enum UserError {
    network(Http.Error);
    invalidResponse;
}

async fn fetchUser(id: Int) -> Result<User, UserError> {
    let response = await Http.get("/api/users/" + id.toString())
        .mapError((error) {
            return UserError.network(error);
        })?;

    let user = Json.decode<User>(response.body)
        .mapError((_) {
            return UserError.invalidResponse;
        })?;

    return .ok(user);
}

component UserScreen {
    state user: Option<User> = .none;
    state loading: Bool = false;
    state error: Option<UserError> = .none;

    async fn load() -> Unit {
        loading.set(true);

        let result = await fetchUser(42);

        match result {
            .ok(value) => {
                user.set(.some(value));
                error.set(.none);
            }

            .err(value) => {
                error.set(.some(value));
            }
        }

        loading.set(false);
    }

    view {
        <Column>
            {match user {
                .some(value) => <Text>{value.name}</Text>;
                .none => <Text>No user loaded</Text>;
            }}

            <Button onPress={load}>
                Load user
            </Button>
        </Column>
    }
}

public fn main() -> App {
    return App {
        root: <UserScreen />,
    };
}
```

The example demonstrates several central Zen properties:

- explicit types;
- immutable values;
- typed errors;
- no null;
- exhaustive pattern matching;
- no exceptions;
- structured asynchronous code;
- framework-managed UI state;
- declarative cross-platform UI;
- native/platform behavior hidden behind typed Zen APIs.

---

# 73. Core language principle

The defining rule of Zen is:

> **If something can fail in ordinary Zen code, that possibility must be visible either to the compiler or in the type system.**

The language should make invalid states difficult or impossible to represent.

The platform should extend that philosophy to application development so that the same Zen source can safely express UI, networking, storage, permissions, location, and other application capabilities across the web, iOS, and Android.

Zen is not intended to hide complexity by making behavior implicit.

It is intended to move complexity downward — into the compiler, runtime, and platform — so application code can remain explicit, small, predictable, and safe.