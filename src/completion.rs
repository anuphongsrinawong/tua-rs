//! 🦀 Rust code completion for the Tua agent.
//!
//! Provides [`CodeCompleter`], a simple prefix-based completer built
//! with a comprehensive list of Rust keywords, types, primitives, and
//! common macros.

/// A prefix-based completer for Rust code.
///
/// Contains a built-in list of Rust keywords, primitive types,
/// common type constructors, and well-known macros.  Words are
/// returned sorted alphabetically.
///
/// # Examples
///
/// ```rust
/// use tua_rs::completion::CodeCompleter;
///
/// let completer = CodeCompleter::new();
/// let results = completer.complete("Vec");
/// assert!(results.contains(&"Vec".to_string()));
///
/// let results = completer.complete("impl");
/// assert!(results.contains(&"impl".to_string()));
///
/// // No matches
/// let results = completer.complete("xyznonexistent");
/// assert!(results.is_empty());
/// ```
pub struct CodeCompleter {
    words: Vec<String>,
}

impl CodeCompleter {
    /// Build a new `CodeCompleter` with a comprehensive set of
    /// Rust built-in words.
    ///
    /// The list includes:
    ///
    /// * **Keywords** — `as`, `async`, `await`, `break`, `const`,
    ///   `continue`, `dyn`, `else`, `enum`, `extern`, `false`, `fn`,
    ///   `for`, `if`, `impl`, `in`, `let`, `loop`, `match`, `mod`,
    ///   `move`, `mut`, `pub`, `ref`, `return`, `self`, `Self`,
    ///   `static`, `struct`, `super`, `trait`, `true`, `type`,
    ///   `unsafe`, `use`, `where`, `while`, `yield`.
    /// * **Primitive types** — `bool`, `char`, `f32`, `f64`, `i8`,
    ///   `i16`, `i32`, `i64`, `i128`, `isize`, `str`, `u8`, `u16`,
    ///   `u32`, `u64`, `u128`, `usize`.
    /// * **Common standard types** — `Box`, `Cell`, `Cow`, `HashMap`,
    ///   `HashSet`, `Mutex`, `Option`, `Rc`, `RefCell`, `Result`,
    ///   `String`, `Vec`, `Arc`, `RwLock`, `BTreeMap`, `BTreeSet`,
    ///   `LinkedList`, `VecDeque`, `BinaryHeap`, `Duration`, `Path`,
    ///   `PathBuf`, `OsString`, `OsStr`, `CString`, `CStr`, `Pin`,
    ///   `UnsafeCell`, `PhantomData`, `ManuallyDrop`, `MaybeUninit`,
    ///   `NonZeroU8`, `NonZeroU16`, `NonZeroU32`, `NonZeroU64`,
    ///   `NonZeroU128`, `NonZeroUsize`, `NonZeroI8`, `NonZeroI16`,
    ///   `NonZeroI32`, `NonZeroI64`, `NonZeroI128`, `NonZeroIsize`.
    /// * **Common macros** — `assert_eq!`, `assert_ne!`, `assert!`,
    ///   `cfg!`, `column!`, `compile_error!`, `concat!`, `dbg!`,
    ///   `debug_assert!`, `debug_assert_eq!`, `debug_assert_ne!`,
    ///   `env!`, `eprint!`, `eprintln!`, `file!`, `format!`,
    ///   `include!`, `include_bytes!`, `include_str!`, `is_x86_feature_detected!`,
    ///   `line!`, `log_syntax!`, `matches!`, `module_path!`,
    ///   `option_env!`, `panic!`, `print!`, `println!`, `stringify!`,
    ///   `thread_local!`, `todo!`, `trace_macros!`, `unimplemented!`,
    ///   `unreachable!`, `vec!`, `write!`, `writeln!`.
    /// * **Prelude re-exports** — `Box`, `String`, `Vec`, `Option`,
    ///   `Result`, `Into`, `From`, `Clone`, `Copy`, `Default`,
    ///   `Iterator`, `IntoIterator`, `ExactSizeIterator`,
    ///   `DoubleEndedIterator`, `Extend`, `AsRef`, `AsMut`, `Borrow`,
    ///   `BorrowMut`, `TryFrom`, `TryInto`, `ToOwned`, `Drop`,
    ///   `Send`, `Sync`, `Sized`, `Unpin`, `ToString`, `Ord`,
    ///   `PartialOrd`, `Eq`, `PartialEq`, `Debug`, `Display`,
    ///   `Hash`, `Default`.
    pub fn new() -> Self {
        let words = Self::builtin_words();
        let mut this = Self { words };
        this.words.sort();
        this.words.dedup();
        this
    }

    /// Return all words that start with the given `prefix` (case-sensitive).
    ///
    /// The returned vector is sorted alphabetically.
    ///
    /// # Examples
    ///
    /// ```
    /// use tua_rs::completion::CodeCompleter;
    ///
    /// let c = CodeCompleter::new();
    /// let r = c.complete("Op");
    /// assert!(r.contains(&"Option".to_string()));
    /// ```
    pub fn complete(&self, prefix: &str) -> Vec<String> {
        if prefix.is_empty() {
            return self.words.clone();
        }

        self.words
            .iter()
            .filter(|w| w.starts_with(prefix))
            .cloned()
            .collect()
    }

    /// Return the total number of built-in words.
    pub fn len(&self) -> usize {
        self.words.len()
    }

    /// Returns `true` if the built-in word list is empty.
    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    // -----------------------------------------------------------------------
    // Built-in word list
    // -----------------------------------------------------------------------

    fn builtin_words() -> Vec<String> {
        let mut words: Vec<&str> = vec![
            // --- Keywords ---
            "as",
            "async",
            "await",
            "break",
            "const",
            "continue",
            "crate",
            "dyn",
            "else",
            "enum",
            "extern",
            "false",
            "fn",
            "for",
            "if",
            "impl",
            "in",
            "let",
            "loop",
            "match",
            "mod",
            "move",
            "mut",
            "pub",
            "ref",
            "return",
            "self",
            "Self",
            "static",
            "struct",
            "super",
            "trait",
            "true",
            "type",
            "unsafe",
            "use",
            "where",
            "while",
            "yield",
            // --- Primitive types ---
            "bool",
            "char",
            "f32",
            "f64",
            "i8",
            "i16",
            "i32",
            "i64",
            "i128",
            "isize",
            "str",
            "u8",
            "u16",
            "u32",
            "u64",
            "u128",
            "usize",
            // --- Common standard types ---
            "Box",
            "Cell",
            "Cow",
            "HashMap",
            "HashSet",
            "Mutex",
            "Option",
            "Rc",
            "RefCell",
            "Result",
            "String",
            "Vec",
            "Arc",
            "RwLock",
            "BTreeMap",
            "BTreeSet",
            "LinkedList",
            "VecDeque",
            "BinaryHeap",
            "Duration",
            "Path",
            "PathBuf",
            "OsString",
            "OsStr",
            "CString",
            "CStr",
            "Pin",
            "UnsafeCell",
            "PhantomData",
            "ManuallyDrop",
            "MaybeUninit",
            "NonZeroU8",
            "NonZeroU16",
            "NonZeroU32",
            "NonZeroU64",
            "NonZeroU128",
            "NonZeroUsize",
            "NonZeroI8",
            "NonZeroI16",
            "NonZeroI32",
            "NonZeroI64",
            "NonZeroI128",
            "NonZeroIsize",
            // --- Common macros ---
            "assert_eq!",
            "assert_ne!",
            "assert!",
            "cfg!",
            "column!",
            "compile_error!",
            "concat!",
            "dbg!",
            "debug_assert!",
            "debug_assert_eq!",
            "debug_assert_ne!",
            "env!",
            "eprint!",
            "eprintln!",
            "file!",
            "format!",
            "include!",
            "include_bytes!",
            "include_str!",
            "is_x86_feature_detected!",
            "line!",
            "log_syntax!",
            "matches!",
            "module_path!",
            "option_env!",
            "panic!",
            "print!",
            "println!",
            "stringify!",
            "thread_local!",
            "todo!",
            "trace_macros!",
            "unimplemented!",
            "unreachable!",
            "vec!",
            "write!",
            "writeln!",
            // --- Prelude traits ---
            "Into",
            "From",
            "Clone",
            "Copy",
            "Default",
            "Iterator",
            "IntoIterator",
            "ExactSizeIterator",
            "DoubleEndedIterator",
            "Extend",
            "AsRef",
            "AsMut",
            "Borrow",
            "BorrowMut",
            "TryFrom",
            "TryInto",
            "ToOwned",
            "Drop",
            "Send",
            "Sync",
            "Sized",
            "Unpin",
            "ToString",
            "Ord",
            "PartialOrd",
            "Eq",
            "PartialEq",
            "Debug",
            "Display",
            "Hash",
        ];

        words.sort();
        words.dedup();
        words.into_iter().map(String::from).collect()
    }
}

impl Default for CodeCompleter {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_complete_keyword() {
        let c = CodeCompleter::new();
        let results = c.complete("impl");
        assert!(results.contains(&"impl".to_string()), "expected 'impl'");
    }

    #[test]
    fn test_complete_prefix() {
        let c = CodeCompleter::new();
        let results = c.complete("Vec");
        assert!(results.contains(&"Vec".to_string()), "expected 'Vec'");
        // Should also match VecDeque
        assert!(
            results.contains(&"VecDeque".to_string()),
            "expected 'VecDeque'"
        );
    }

    #[test]
    fn test_complete_no_match() {
        let c = CodeCompleter::new();
        let results = c.complete("zzzzz");
        assert!(results.is_empty());
    }

    #[test]
    fn test_complete_empty_prefix_returns_all() {
        let c = CodeCompleter::new();
        let results = c.complete("");
        assert!(results.len() >= 100, "expected at least 100 built-in words");
    }

    #[test]
    fn test_len_and_is_empty() {
        let c = CodeCompleter::new();
        assert!(c.len() > 0);
        assert!(!c.is_empty());
    }

    #[test]
    fn test_default() {
        let c1 = CodeCompleter::new();
        let c2 = CodeCompleter::default();
        assert_eq!(c1.len(), c2.len());
    }

    #[test]
    fn test_no_duplicates() {
        let c = CodeCompleter::new();
        let mut sorted = c.words.clone();
        sorted.dedup();
        assert_eq!(
            c.words.len(),
            sorted.len(),
            "words must not contain duplicates"
        );
    }

    #[test]
    fn test_sorted_output() {
        let c = CodeCompleter::new();
        let results = c.complete("S");
        let mut sorted = results.clone();
        sorted.sort();
        assert_eq!(results, sorted, "completion results must be sorted");
    }
}
