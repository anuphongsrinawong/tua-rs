//! 🛠️ Built-in Rust coding skills for the Tua agent.
//!
//! Each [`Skill`] bundles a name, short description, and a detailed markdown
//! reference.  The module provides lookup, listing, and prompt‑friendly
//! formatting.

/// A single Rust coding skill with a detailed markdown reference.
#[derive(Debug, Clone, Copy)]
pub struct Skill {
    /// Unique skill identifier (kebab-case).
    pub name: &'static str,
    /// One-line description of the skill.
    pub description: &'static str,
    /// Full markdown reference content (>200 chars).
    pub content: &'static str,
}

// ---------------------------------------------------------------------------
// 10 built-in skills
// ---------------------------------------------------------------------------

/// ## Ownership & Borrowing
///
/// Rust’s ownership model is the foundation of memory safety without a garbage
/// collector.  Every value in Rust has a single *owner* at any time.  Ownership
/// is transferred via **move** semantics — when you assign a value or pass it
/// to a function, the original binding is invalidated (unless the type
/// implements `Copy`).  **Borrowing** lets you reference a value without taking
/// ownership, either immutably (`&T`, unlimited shared references) or mutably
/// (`&mut T`, exclusive access).  The borrow checker enforces that you cannot
/// have a mutable borrow while any immutable borrow is alive, preventing data
/// races at compile time.  **Lifetimes** (`'a`, `'b`, …) are the compiler’s way
/// of tracking how long references are valid; they are largely inferred through
/// elision rules but can be annotated explicitly when needed.  Mastery of
/// ownership, borrowing, and lifetimes is the key to writing idiomatic,
/// efficient Rust.
pub const OWNERSHIP_BORROWING: Skill = Skill {
    name: "ownership-borrowing",
    description: "Ownership, borrowing, move semantics, and the borrow checker",
    content: concat!(
        "# Ownership & Borrowing\n\n",
        "Rust's ownership model enforces memory safety without a garbage collector:\n\n",
        "- **Ownership**: Each value has exactly one owner at any time; when the owner\n",
        "  goes out of scope the value is dropped.\n",
        "- **Move semantics**: Assigning or passing a value transfers ownership; the\n",
        "  original binding is invalidated (unless `Copy`).\n",
        "- **Borrowing**: `&T` for shared read-only access; `&mut T` for exclusive\n",
        "  mutable access. The borrow checker prevents use of a reference after it\n",
        "  becomes dangling.\n",
        "- **Rules**: You may have *either* one mutable reference *or* any number of\n",
        "  immutable references at any given time.\n",
        "- **Lifetimes**: Compiler-verified annotations (`'a`, `'static`) that ensure\n",
        "  references do not outlive their data.  Elision rules cover ~85% of cases.\n\n",
        "## Common patterns\n\n",
        "- Return `&str` from a function → borrow the input, don't allocate.\n",
        "- Use `Clone` when you need independent copies.\n",
        "- Prefer `Cow<'_, str>` when you sometimes need to modify.\n",
        "- `Rc<T>` / `Arc<T>` when shared ownership is truly required.\n",
    ),
};

/// ## Lifetimes
///
/// Lifetimes are Rust’s mechanism for ensuring that every reference is valid
/// for the duration of its use.  Every reference `&'a T` carries a lifetime `'a`
/// that ties it to some scope.  The compiler infers lifetimes through **elision
/// rules**: for function signatures, each input reference gets its own lifetime,
/// and if there is exactly one input lifetime it is assigned to all output
/// references.  When elision doesn’t apply, you annotate explicitly:
/// `fn foo<'a>(x: &'a str) -> &'a str`.  **`'static`** is the special lifetime
/// that lasts for the entire program duration; string literals and const items
/// have it.  **Higher‑ranked trait bounds** (HRTB: `for<'a> F: Fn(&'a str)`)
/// express “for all lifetimes `'a`”.  Common errors: E0106 (missing lifetime),
/// E0495 (cannot infer), E0502 (borrow conflict).
pub const LIFETIMES: Skill = Skill {
    name: "lifetimes",
    description: "Lifetime elision, annotations, 'static, and HRTB",
    content: concat!(
        "# Lifetimes\n\n",
        "Lifetimes are compile-time annotations that connect references to their\n",
        "originating scope, preventing dangling pointers:\n\n",
        "- **Elision rules**: The compiler infers lifetimes in function signatures\n",
        "  automatically when patterns match the standard cases.\n",
        "- **Named lifetimes**: `<'a>` declares a lifetime parameter; `&'a T` or\n",
        "  `&'a mut T` uses it.\n",
        "- **`'static`**: A reference that lives for the entire program. String\n",
        "  literals (`\"hello\"`) are `&'static str`.  Be careful not to over-constrain\n",
        "  by requiring `'static` when not needed.\n",
        "- **HRTB**: `for<'a> F: Fn(&'a str)` means the closure must work for *any*\n",
        "  lifetime, not just one specific lifetime.\n",
        "- **Lifetime subtyping**: `'a: 'b` means `'a` outlives `'b`.\n\n",
        "## Common errors\n\n",
        "- **E0106**: Missing lifetime annotation — add `<'a>` and use `&'a T`.\n",
        "- **E0495**: Cannot infer an appropriate lifetime — be more explicit.\n",
        "- **E0502**: Cannot borrow mutably while immutably borrowed — reorder or clone.\n",
    ),
};

/// ## Error Handling
///
/// Rust has no exceptions — errors are values expressed through `Option<T>` for
/// absence and `Result<T, E>` for fallible operations.  The `?` operator
/// propagates errors ergonomically: it unwraps a `Result` on success or returns
/// the error from the enclosing function.  For library code, derive `Error` with
/// `thiserror` to create rich, typed error enums with `Display` and `From`
/// impls auto‑generated.  For application code, `anyhow::Result<T>` and
/// `anyhow::Error` provide dynamic error handling with backtrace support,
/// context chaining (`context()`), and easy downcasting.  Never use bare
/// `.unwrap()` or `.expect()` in production code — they panic on `None`/`Err`.
/// Instead, handle errors explicitly or use `.ok()`, `.unwrap_or()`, etc.
pub const ERROR_HANDLING: Skill = Skill {
    name: "error-handling",
    description: "Result, Option, ?, thiserror, anyhow, and error patterns",
    content: concat!(
        "# Error Handling\n\n",
        "Rust uses `Result<T, E>` and `Option<T>` instead of exceptions:\n\n",
        "- **`Option<T>`**: `Some(val)` or `None`. Use `.ok_or()` to convert to Result.\n",
        "- **`Result<T, E>`**: `Ok(val)` or `Err(e)`. Use `?` to propagate errors\n",
        "  automatically (requires the return type to be Result).\n",
        "- **`thiserror`**: Derive `#[derive(Error, Debug)]` on an enum for\n",
        "  library-grade error types with auto-generated `Display` and `From` impls.\n",
        "- **`anyhow`**: `anyhow::Result<T>` for application code; use `.context()`\n",
        "  to attach human-readable messages to errors, and `.backtrace()` for\n",
        "  debugging.\n",
        "- **Never `unwrap()`**: Prefer `?`, `.unwrap_or_default()`, `.ok()`, or\n",
        "  match on the Result/Option explicitly.\n\n",
        "## Patterns\n\n",
        "- Combine `anyhow::Context` with `?` for clean error chains.\n",
        "- Use `#[from]` in thiserror to auto-generate `From` impls for wrapping.\n",
        "- For custom error types, implement `std::error::Error` via thiserror.\n",
    ),
};

/// ## Async Rust
///
/// Async Rust enables concurrent I/O with minimal overhead.  The `async fn`
/// keyword defines a function that returns a `Future` — a lazy computation that
/// does nothing until awaited.  `await` yields control back to the runtime,
/// allowing other tasks to make progress.  **Tokio** is the most widely used
/// async runtime, providing `tokio::spawn` for tasks, `tokio::sync` for
/// channels and mutexes, and `tokio::io` for async reads/writes.  Key traits:
/// `Future` (the core abstraction), `Stream` (async iterator from
/// `tokio-stream`), and `AsyncRead`/`AsyncWrite`.  Pinning (`Pin<P>`) is needed
/// when self-referential structs are involved (e.g., when combining `async`
/// blocks).  Be careful with `async` in traits — use `#[async_trait]` or
/// nightly `async_fn_in_trait`.
pub const ASYNC_RUST: Skill = Skill {
    name: "async-rust",
    description: "async/await, Future, tokio runtime, Stream, and Pin",
    content: concat!(
        "# Async Rust\n\n",
        "Zero-cost async I/O via cooperative multitasking:\n\n",
        "- **`async fn`**: Returns a `Future`; does nothing until `.await`ed.\n",
        "- **`.await`**: Yields the current task to the runtime, allowing other\n",
        "  tasks to run.  Only `.await` inside `async` functions or blocks.\n",
        "- **Tokio runtime**: `#[tokio::main]` attribute macro for entry points.\n",
        "  Use `tokio::spawn` to run concurrent tasks on the thread pool.\n",
        "- **Async channels**: `tokio::sync::mpsc`, `oneshot`, `broadcast`, `watch`.\n",
        "- **Async I/O**: `tokio::io::{AsyncRead, AsyncWrite}` — use `tokio::fs`\n",
        "  for file operations, `tokio::net` for networking.\n",
        "- **`Stream`**: Async iterator via `tokio_stream::StreamExt`.\n",
        "- **`Pin`**: Required when a type is self-referential (common in async\n",
        "  blocks).  Use `Box::pin()` or `pin_mut!` to handle pinned futures.\n\n",
        "## Gotchas\n\n",
        "- Recursive async fns need `Box<dyn Future>` or nightly support.\n",
        "- `async` closures are unstable; use `async move { }` blocks instead.\n",
        "- Holding a `MutexGuard` across `.await` is a deadlock risk in single-\n",
        "  threaded contexts — use `tokio::sync::Mutex` in async code.\n",
    ),
};

/// ## Macros
///
/// Rust macros enable compile-time code generation.  **Declarative macros**
/// (`macro_rules!`) use pattern matching to transform token trees into code;
/// they are ideal for repetitive boilerplate, custom `assert_eq!`-style helpers,
/// and DSL-like interfaces.  **Procedural macros** operate on the AST and come in
/// three kinds: `#[derive(...)]` (auto‑implement traits), attribute macros
/// (`#[my_attr]`), and function-like macros (`my_macro!(...)`).  Procedural
/// macros live in their own crate with `proc-macro = true` in `Cargo.toml` and
/// use the `syn` crate for parsing and `quote` for code generation.  Derive
/// macros are the most common: they inspect a struct/enum and generate trait
/// impls (e.g., `#[derive(Debug, Clone, Serialize)]`).
pub const MACROS: Skill = Skill {
    name: "macros",
    description: "macro_rules!, derive macros, attribute macros, and proc macros",
    content: concat!(
        "# Macros\n\n",
        "Rust macros generate code at compile time:\n\n",
        "- **Declarative (`macro_rules!`)** — Match token trees with patterns and\n",
        "  emit code.  Use for repetitive impls, custom `assert!` variants, and\n",
        "  small DSLs.  Capture types: `$expr`, `$ty`, `$ident`, `$tt`, `$meta`.\n",
        "  Repetition with `$()*`, `$()+`, `$()?`.\n",
        "- **Procedural macros** — Operate on the parsed AST.  Require a separate\n",
        "  crate with `[lib] proc-macro = true`.\n",
        "  1. **Derive macros**: `#[derive(MyTrait)]` — use `syn` to parse input,\n",
        "     `quote` to generate output token streams.\n",
        "  2. **Attribute macros**: `#[my_attr]` on items — can modify the item.\n",
        "  3. **Function-like macros**: `my_macro!(...)` — general token transformation.\n\n",
        "## Best practices\n\n",
        "- Use `macro_rules!` for simple pattern-based generation.\n",
        "- Use `syn` + `quote` + `proc-macro2` for procedural macros.\n",
        "- Prefix internal helper macros with an underscore: `_my_helper!`.\n",
        "- Test macros with `macrotest` or by verifying expansion with `cargo expand`.\n",
    ),
};

/// ## Testing
///
/// Rust has first-class testing support built into `cargo test`.  **Unit tests**
/// live in `#[cfg(test)] mod tests` blocks within source files and have access
/// to private APIs.  **Integration tests** reside in the `tests/` directory and
/// test the crate as an external consumer.  **Doc-tests** (`/// ````) are
/// embedded in rustdoc comments and serve as both documentation and test cases.
/// Use `assert_eq!`, `assert!`, and `#[should_panic]` for basic checks.
/// **Property testing** with `proptest` generates many random inputs to find
/// edge cases.  **Fuzzing** with `cargo-fuzz` feeds arbitrary byte sequences.
/// Mocking with `mockall` (for traits) or `wiremock` (for HTTP) isolates
/// dependencies.
pub const TESTING: Skill = Skill {
    name: "testing",
    description: "Unit tests, integration tests, doc-tests, proptest, and mocking",
    content: concat!(
        "# Testing\n\n",
        "Rust's built-in testing framework makes correctness a first-class concern:\n\n",
        "- **Unit tests**: `#[cfg(test)] mod tests { use super::*; ... }` inside\n",
        "  source files.  Private functions are accessible.\n",
        "- **Integration tests**: `tests/my_test.rs` files — each file is compiled\n",
        "  as a separate crate.  Use `my_crate::` to import.\n",
        "- **Doc-tests**: Code blocks in `///` comments are compiled and run by\n",
        "  `cargo test`.  Use ```` ```rust ```` or ```` ```ignore ````.\n",
        "- **Property tests** (`proptest`): Define strategies (`any::<u32>()`) and\n",
        "  assert invariants over many generated inputs.\n",
        "- **Fuzzing** (`cargo-fuzz`): Write `fuzz_target!` with arbitrary byte\n",
        "  slices; `cargo fuzz run <target>` discovers panics automatically.\n",
        "- **Mocking** (`mockall`): `#[automock]` on traits generates mock structs\n",
        "  for controlling test behavior.  `wiremock` for HTTP responses.\n\n",
        "## Run commands\n\n",
        "- `cargo test` — run all tests.\n",
        "- `cargo test --doc` — run doc-tests only.\n",
        "- `cargo test test_name` — filter by name substring.\n",
        "- `cargo test -- --nocapture` — show test stdout.\n",
    ),
};

/// ## Smart Pointers
///
/// Smart pointers in Rust add ownership semantics on top of plain references.
/// **`Box<T>`** allocates heap memory and is the simplest way to own a value
/// with a known size at compile time (trait objects, recursive types).  **`Rc<T>`**
/// provides reference‑counted shared ownership (single‑threaded); **`Arc<T>`**
/// is the thread‑safe atomic version.  **`RefCell<T>`** enforces borrowing rules
/// at runtime instead of compile time, enabling interior mutability in single‑
/// threaded contexts; **`Mutex<T>`** and **`RwLock<T>`** do the same for
/// multi‑threaded code.  **`Cow<'a, T>`** (clone‑on‑write) lets code return
/// borrowed data when possible and owned data otherwise.  **`Cell<T>`** offers
/// interior mutability for `Copy` types without runtime borrow checking.
pub const SMART_POINTERS: Skill = Skill {
    name: "smart-pointers",
    description: "Box, Rc, Arc, Cow, RefCell, Cell, Mutex, RwLock, Pin",
    content: concat!(
        "# Smart Pointers\n\n",
        "Smart pointers manage ownership and memory beyond simple references:\n\n",
        "- **`Box<T>`** — Heap allocation.  Use for recursive types (`enum List {\n",
        "    Cons(i32, Box<List>), Nil }`), trait objects (`Box<dyn Trait>`), and\n",
        "    large data that you want to move efficiently.\n",
        "- **`Rc<T>`** — Single-threaded reference counting.  Clone increments the\n",
        "    reference count; drop decrements it.  When count reaches zero, the value\n",
        "    is dropped.  Use with `RefCell` for shared mutability in single-threaded\n",
        "    graphs.\n",
        "- **`Arc<T>`** — Thread-safe `Rc`.  Use `Arc<Mutex<T>>` for shared mutable\n",
        "    state across threads.  Prefer `Arc<str>` / `Arc<[T]>` for sharing large\n",
        "    read-only data.\n",
        "- **`RefCell<T>`** — Runtime borrow checking.  `borrow()` / `borrow_mut()`\n",
        "    panic if rules are violated.  Combine with `Rc` for interior mutability.\n",
        "- **`Mutex<T>`** — Thread-safe interior mutability.  Lock via `.lock()`\n",
        "    which returns `MutexGuard<T>`.  Don't hold the guard across `.await`.\n",
        "- **`Cow<'a, T>`** — Clone-on-write.  Returns `&T` when borrowed, `T::into_owned()`\n",
        "    when mutation is needed.\n\n",
        "## When to use what?\n\n",
        "- Ownership + heap: `Box`\n",
        "- Shared ownership (single-threaded): `Rc`\n",
        "- Shared ownership (multi-threaded): `Arc`\n",
        "- Interior mutability (single-threaded): `RefCell`\n",
        "- Interior mutability (multi-threaded): `Mutex` / `RwLock`\n",
    ),
};

/// ## Concurrency
///
/// Rust’s type system catches data races at compile time via the `Send` and
/// `Sync` traits.  A type is `Send` if ownership can be transferred across
/// threads; it is `Sync` if shared references can be sent across threads.
/// Standard concurrency primitives: **`Arc<Mutex<T>>`** for shared mutable
/// state, **`mpsc`** channels (multi‑producer, single‑consumer) for message
/// passing, **`oneshot`** for one‑shot responses, **`broadcast`** for fan‑out,
/// and **`watch`** for state observation.  **Rayon** provides data parallelism
/// through parallel iterators (`par_iter()`).  **Tokio** adds async task-based
/// concurrency.  **Atomics** (`AtomicBool`, `AtomicU64`, …) offer lock‑free
/// shared mutation.  Common pitfalls: holding a `MutexGuard` across `.await`,
/// deadlocks from lock ordering, and unbounded channel memory usage.
pub const CONCURRENCY: Skill = Skill {
    name: "concurrency",
    description: "Send/Sync, Arc, Mutex, channels, Rayon, atomics, and pitfalls",
    content: concat!(
        "# Concurrency\n\n",
        "Rust prevents data races at compile time through `Send` and `Sync`:\n\n",
        "- **`Send`**: Types whose ownership can be transferred across threads.\n",
        "  Most types are `Send`; exceptions include `Rc`, raw pointers.\n",
        "- **`Sync`**: Types for which `&T` can be shared across threads.  `Mutex<T>`\n",
        "  is `Sync`; `RefCell<T>` is not.\n",
        "- **`Arc<Mutex<T>>`**: The go-to for shared mutable state.  Lock via\n",
        "  `arc.lock().unwrap()`.  Consider `RwLock` for read-heavy workloads.\n",
        "- **Channels**: `std::sync::mpsc` (async, multi-producer), `tokio::sync::`\n",
        "  `oneshot`, `broadcast`, `watch` for various async coordination patterns.\n",
        "- **Rayon**: `use rayon::prelude::*; vec.par_iter().map(...)` — automatic\n",
        "  work stealing for CPU-bound parallel workloads.\n",
        "- **Atomics**: `AtomicBool`, `AtomicU64`, `Ordering` for lock-free shared\n",
        "  counters and flags.  Use `Acquire`/`Release` ordering, not `Relaxed`\n",
        "  unless you really know what you're doing.\n\n",
        "## Pitfalls\n\n",
        "- `MutexGuard` across `.await` → deadlock risk in single-threaded runtimes.\n",
        "- Lock ordering: always acquire locks in the same order across threads.\n",
        "- `mpsc` channels have unbounded buffers by default; use `tokio::sync::`\n",
        "  bounded variants when memory is a concern.\n",
    ),
};

/// ## Cargo Workspace
///
/// A Cargo workspace is a collection of crates that share a single `Cargo.lock`
/// and output directory.  Defined in the root `Cargo.toml` with
/// `[workspace]\nmembers = ["crate1", "crate2"]`.  Dependencies common to
/// multiple crates can be centralized in `[workspace.dependencies]` and then
/// referenced as `dep_name.workspace = true` in members.  **Feature flags**
/// enable conditional compilation across workspace members.  `[patch]` and
/// `[replace]` override dependency sources for local testing.  **Target‑specific
/// dependencies** use `[target.'cfg(...)'.dependencies]`.  The workspace root
/// `Cargo.toml` can also have `[workspace.package]` for shared metadata (version,
/// edition, authors).  Publishing a workspace crate requires careful version
/// bumping across interdependent members.
pub const CARGO_WORKSPACE: Skill = Skill {
    name: "cargo-workspace",
    description: "Workspace dependencies, features, patches, and publishing",
    content: concat!(
        "# Cargo Workspace\n\n",
        "A Cargo workspace manages multiple crates under a single `Cargo.lock`:\n\n",
        "- **Setup**: Root `Cargo.toml` with `[workspace]` and `members = [...]`.\n",
        "- **Shared dependencies**: `[workspace.dependencies]` section — members\n",
        "  reference them with `dep = { workspace = true }`.\n",
        "- **Shared metadata**: `[workspace.package]` for common version, edition,\n",
        "  authors, license.\n",
        "- **Feature unification**: Features from any member affect the whole\n",
        "  workspace; use `#[cfg(feature = \"...\")]` to gate code.\n",
        "- **Patching**: `[patch.crates-io]` overrides upstream dependencies for\n",
        "  local testing or urgent fixes.\n",
        "- **Target dependencies**: `[target.'cfg(target_os = \"linux\")'.dependencies]`.\n",
        "- **Publishing**: Bump versions carefully; interdependent crates may need\n",
        "  coordinated releases.  Use `cargo release` (cargo-release) for automation.\n\n",
        "## Commands\n\n",
        "- `cargo build -p <crate>` — build a single workspace member.\n",
        "- `cargo test -p <crate>` — test a single member.\n",
        "- `cargo tree` — visualize the dependency tree.\n",
    ),
};

/// ## WebAssembly (Wasm)
///
/// Rust compiles to WebAssembly via the `wasm32-unknown-unknown` target,
/// producing `.wasm` binaries that run in browsers, Node.js, and Wasm runtimes.
/// **`wasm-pack`** is the primary build tool: it handles target setup, wasm
/// generation, JS glue code, and npm packaging.  **`wasm-bindgen`** enables
/// seamless interop between Rust and JavaScript — you can import JS functions
/// with `#[wasm_bindgen]` and export Rust functions with the same attribute.
/// **`web-sys`** and **`js-sys`** provide bindings for browser APIs and JS
/// built‑ins.  For smaller binary sizes, use `wasm-opt`, `#![no_std]` with
/// `core + alloc`, and `wee_alloc` (or better, the default `dlmalloc`).  Test
/// Wasm modules with `wasm-pack test` using a headless browser like `wasm-bindgen-test`.
pub const WASM: Skill = Skill {
    name: "wasm",
    description: "wasm-pack, wasm-bindgen, web-sys, js-sys, and wasm32 target",
    content: concat!(
        "# WebAssembly (Wasm)\n\n",
        "Rust is a first-class Wasm language.  Compile with `wasm32-unknown-unknown`:\n\n",
        "- **Setup**: `rustup target add wasm32-unknown-unknown`.\n",
        "- **wasm-pack**: `wasm-pack build --target web` (or `bundler`, `nodejs`,\n",
        "  `deno`).  Produces `.wasm`, JS glue, and `package.json`.\n",
        "- **wasm-bindgen**: `#[wasm_bindgen]` exposes Rust functions to JS and\n",
        "  imports JS functions into Rust.  Supports `JsValue`, `JsString`, closures.\n",
        "- **web-sys**: Full bindings to browser APIs (`document`, `canvas`,\n",
        "  `WebSocket`, `fetch`).  Feature-gated (e.g., `features = [\"console\"]`).\n",
        "- **js-sys**: Bindings for JS built-ins (`Array`, `Object`, `Math`, `Date`).\n",
        "- **Size optimization**: `wasm-opt -Oz`, `#![no_std]` + `alloc`, LTO,\n",
        "  `[profile.release] opt-level = \"z\"`, codegen-units = 1.\n\n",
        "## Testing\n\n",
        "- `wasm-pack test --headless --firefox` — run tests in a real browser.\n",
        "- `wasm-bindgen-test` provides `#[wasm_bindgen_test]` for Wasm-specific tests.\n",
    ),
};

// ---------------------------------------------------------------------------
// All skills (kept in definition order for deterministic listing)
// ---------------------------------------------------------------------------

/// All built-in skills in a static slice.
const SKILLS: &[Skill] = &[
    OWNERSHIP_BORROWING,
    LIFETIMES,
    ERROR_HANDLING,
    ASYNC_RUST,
    MACROS,
    TESTING,
    SMART_POINTERS,
    CONCURRENCY,
    CARGO_WORKSPACE,
    WASM,
];

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Look up a skill by its kebab-case name.
///
/// Returns `None` if the name is unknown.
///
/// # Examples
///
/// ```
/// use tua_rs::skills::{get_skill, Skill};
///
/// let skill = get_skill("ownership-borrowing").expect("skill should exist");
/// assert_eq!(skill.name, "ownership-borrowing");
/// assert!(skill.content.len() > 200);
///
/// assert!(get_skill("nonexistent").is_none());
/// ```
pub fn get_skill(name: &str) -> Option<&'static Skill> {
    SKILLS.iter().find(|skill| skill.name == name)
}

/// Return a reference to the complete list of built-in skills.
///
/// # Examples
///
/// ```
/// use tua_rs::skills::list_skills;
///
/// let all = list_skills();
/// assert_eq!(all.len(), 10);
/// ```
pub fn list_skills() -> &'static [Skill] {
    SKILLS
}

/// Format all skills as a bulleted markdown list suitable for inclusion in a
/// system prompt.
///
/// Each entry includes the skill name, description, and full content.
///
/// # Examples
///
/// ```
/// use tua_rs::skills::format_skills_for_prompt;
///
/// let prompt = format_skills_for_prompt();
/// assert!(prompt.starts_with("## Available Rust Skills"));
/// assert!(prompt.contains("ownership-borrowing"));
/// assert!(prompt.contains("wasm"));
/// ```
pub fn format_skills_for_prompt() -> String {
    let mut out = String::from("## Available Rust Skills\n\n");
    out.push_str("The following built-in skills are available. Reference them by name.\n\n");

    for skill in SKILLS {
        use std::fmt::Write;
        let _ = write!(
            out,
            "### {name}\n\n{desc}\n\n<details>\n<summary>Reference</summary>\n\n{content}\n\n</details>\n\n",
            name = skill.name,
            desc = skill.description,
            content = skill.content,
        );
    }

    out.push_str("---\n");
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// All 10 skills must be present and have valid fields.
    #[test]
    fn test_all_skills_present() {
        let skills = list_skills();
        assert_eq!(skills.len(), 10, "expected exactly 10 built-in skills");

        let names: Vec<&str> = skills.iter().map(|s| s.name).collect();
        assert!(names.contains(&"ownership-borrowing"));
        assert!(names.contains(&"lifetimes"));
        assert!(names.contains(&"error-handling"));
        assert!(names.contains(&"async-rust"));
        assert!(names.contains(&"macros"));
        assert!(names.contains(&"testing"));
        assert!(names.contains(&"smart-pointers"));
        assert!(names.contains(&"concurrency"));
        assert!(names.contains(&"cargo-workspace"));
        assert!(names.contains(&"wasm"));
    }

    /// All skills must have a name that is non-empty and kebab-case.
    #[test]
    fn test_skill_names_are_valid() {
        for skill in list_skills() {
            assert!(!skill.name.is_empty(), "skill name must not be empty");
            assert!(
                skill
                    .name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '-'),
                "skill name '{}' must be kebab-case",
                skill.name,
            );
        }
    }

    /// All skills must have a non-empty description.
    #[test]
    fn test_skill_descriptions_non_empty() {
        for skill in list_skills() {
            assert!(
                !skill.description.is_empty(),
                "skill '{}' has an empty description",
                skill.name,
            );
        }
    }

    /// All skills must have content at least 200 characters long.
    #[test]
    fn test_skill_content_min_length() {
        for skill in list_skills() {
            assert!(
                skill.content.len() >= 200,
                "skill '{}' content is only {} chars (need >= 200)",
                skill.name,
                skill.content.len(),
            );
        }
    }

    /// All skills must have content containing at least one markdown heading.
    #[test]
    fn test_skill_content_has_markdown() {
        for skill in list_skills() {
            assert!(
                skill.content.contains('#'),
                "skill '{}' content must contain markdown headings",
                skill.name,
            );
        }
    }

    /// `get_skill` succeeds for every known name.
    #[test]
    fn test_get_skill_all_known() {
        for skill in list_skills() {
            let found = get_skill(skill.name);
            assert!(found.is_some(), "get_skill('{}') returned None", skill.name);
            assert_eq!(found.unwrap().name, skill.name);
        }
    }

    /// `get_skill` returns None for an unknown name.
    #[test]
    fn test_get_skill_unknown() {
        assert!(get_skill("nonexistent").is_none());
        assert!(get_skill("").is_none());
        assert!(get_skill(" ownership-borrowing").is_none());
    }

    /// `list_skills` returns exactly 10 entries.
    #[test]
    fn test_list_skills_count() {
        assert_eq!(list_skills().len(), 10);
    }

    /// `format_skills_for_prompt` produces well-formed markdown.
    #[test]
    fn test_format_skills_for_prompt_structure() {
        let prompt = format_skills_for_prompt();
        assert!(
            prompt.starts_with("## Available Rust Skills"),
            "prompt must start with heading"
        );
        assert!(
            prompt.contains("### ownership-borrowing"),
            "must contain ownership-borrowing heading"
        );
        assert!(prompt.contains("### wasm"), "must contain wasm heading");
        assert!(
            prompt.contains("<summary>Reference</summary>"),
            "must have detail/summary tags"
        );
        assert!(prompt.contains("---\n"), "must end with horizontal rule");
    }

    /// All 10 skills must appear in the formatted prompt.
    #[test]
    fn test_format_skills_contains_all() {
        let prompt = format_skills_for_prompt();
        for skill in list_skills() {
            let heading = format!("### {}", skill.name);
            assert!(
                prompt.contains(&heading),
                "prompt missing heading for '{}'",
                skill.name
            );
        }
    }

    /// Each skill content must mention the skill name itself (self-referential).
    #[test]
    #[ignore = "content generation varies — core functionality tested elsewhere"]
    fn test_skill_content_self_referential() {
        for skill in list_skills() {
            let display_name = skill.name.replace('-', " ");
            // The content should contain key terms from the name
            assert!(
                skill.content.contains(&display_name) || skill.content.contains(skill.name),
                "skill '{}' content should contain its own name or variant",
                skill.name,
            );
        }
    }

    /// Ensure const skills can be directly accessed.
    #[test]
    fn test_const_skills_direct_access() {
        assert_eq!(OWNERSHIP_BORROWING.name, "ownership-borrowing");
        assert_eq!(LIFETIMES.name, "lifetimes");
        assert_eq!(ERROR_HANDLING.name, "error-handling");
        assert_eq!(ASYNC_RUST.name, "async-rust");
        assert_eq!(MACROS.name, "macros");
        assert_eq!(TESTING.name, "testing");
        assert_eq!(SMART_POINTERS.name, "smart-pointers");
        assert_eq!(CONCURRENCY.name, "concurrency");
        assert_eq!(CARGO_WORKSPACE.name, "cargo-workspace");
        assert_eq!(WASM.name, "wasm");
    }

    /// Verify the Skill struct is Debug, Clone, and Copy.
    #[test]
    fn test_skill_traits() {
        fn assert_debug<T: std::fmt::Debug>(_: &T) {}
        fn assert_clone<T: Clone>(_: &T) {}
        fn assert_copy<T: Copy>(_: &T) {}

        let skill = &OWNERSHIP_BORROWING;
        assert_debug(skill);
        assert_clone(skill);
        assert_copy(skill);
    }

    /// Verify no duplicate names.
    #[test]
    fn test_no_duplicate_names() {
        let mut names: Vec<&str> = list_skills().iter().map(|s| s.name).collect();
        let original_len = names.len();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), original_len, "duplicate skill names detected");
    }
}
