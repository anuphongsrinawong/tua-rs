//! Test generator — parse Rust source files and generate `#[test]` skeletons.
//!
//! Given a path to a `.rs` file, this module finds every `pub fn` signature
//! and emits a `#[test]` skeleton covering:
//!
//! * A basic happy-path invocation (with dummy arguments where possible)
//! * Edge cases (empty inputs, maximum values, `None` for `Option` params)
//! * Error paths (when the function returns `Result`)
//!
//! # Example
//!
//! ```rust
//! use tua_rs::testgen;
//!
//! let tests = testgen::generate_tests("src/example.rs");
//! for t in &tests {
//!     println!("{t}\n");
//! }
//! ```

use std::fs;
use std::path::Path;

/// A parsed function signature extracted from Rust source.
#[derive(Debug, Clone)]
pub struct FnSignature {
    /// The function name.
    pub name: String,
    /// The full signature line (e.g. `pub fn foo(x: u32) -> Result<(), Error>`).
    pub signature: String,
    /// Parameter names.
    pub params: Vec<ParamInfo>,
    /// Return type (if any).
    pub return_type: Option<String>,
    /// Whether the function returns `Result<...>`.
    pub returns_result: bool,
    /// Whether the function returns `Option<...>`.
    pub returns_option: bool,
    /// Whether the function is `async`.
    pub is_async: bool,
    /// The line number where the function starts.
    pub line: usize,
}

/// Info about a single function parameter.
#[derive(Debug, Clone)]
pub struct ParamInfo {
    /// Parameter name.
    pub name: String,
    /// The type string (e.g. `&str`, `u32`, `Vec<String>`).
    pub type_str: String,
}

/// Generate `#[test]` skeletons for every `pub fn` in `source_path`.
///
/// Each returned string is a complete test function that can be
/// pasted into a `#[cfg(test)] mod tests { ... }` block.
pub fn generate_tests(source_path: &str) -> Vec<String> {
    let path = Path::new(source_path);
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let mod_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");

    let sigs = parse_fns(&content);
    let mut tests = Vec::with_capacity(sigs.len() * 3);

    for sig in &sigs {
        tests.push(generate_happy_path_test(sig, mod_name));
        tests.push(generate_edge_case_test(sig, mod_name));
        if sig.returns_result {
            tests.push(generate_error_test(sig, mod_name));
        }
    }

    tests
}

fn generate_happy_path_test(sig: &FnSignature, mod_name: &str) -> String {
    let test_name = format!("test_{}_happy_path", sig.name);
    let param_values = generate_dummy_values(&sig.params);
    let call = build_call(sig, &param_values);

    let async_attr = if sig.is_async {
        "\n#[tokio::test]"
    } else {
        ""
    };
    let await_kw = if sig.is_async { ".await" } else { "" };

    let mut body = String::new();
    for (name, val) in &param_values {
        body.push_str(&format!("    let {name} = {val};\n"));
    }
    body.push_str(&format!("    // Act\n    let result = {}{};\n", call, await_kw));

    if sig.returns_result {
        body.push_str(
            "    // Assert\n    assert!(result.is_ok(), \"expected Ok, got: {:?}\", result);\n",
        );
    } else if sig.returns_option {
        body.push_str(
            "    // Assert\n    assert!(result.is_some(), \"expected Some, got None\");\n",
        );
    } else {
        body.push_str("    // Assert — add expected behaviour here\n");
    }

    format!(
        "/// Happy-path test for `{mod_name}::{fname}`.\n\
         #[test]{async_attr}\n\
         fn {test_name}() {{\n\
         {body}}}\n",
        fname = sig.name,
        test_name = test_name,
        async_attr = async_attr,
        body = body,
    )
}

fn generate_edge_case_test(sig: &FnSignature, mod_name: &str) -> String {
    let test_name = format!("test_{}_edge_cases", sig.name);
    let async_attr = if sig.is_async {
        "\n#[tokio::test]"
    } else {
        ""
    };

    let mut body = String::from("    // Edge case: empty / zero / None values\n");
    for param in &sig.params {
        let edge_val = edge_value(&param.type_str);
        body.push_str(&format!(
            "    let {} = {}; // {}\n",
            param.name, edge_val, param.type_str
        ));
    }

    let call = build_call(sig, &generate_edge_values(&sig.params));
    let await_kw = if sig.is_async { ".await" } else { "" };

    body.push_str(&format!(
        "    // Act — should handle gracefully\n    let _result = {}{};\n",
        call, await_kw
    ));
    if sig.returns_result {
        body.push_str("    // let _ = result; // ignore or assert with is_err()/is_ok()\n");
    }

    format!(
        "/// Edge-case test for `{mod_name}::{fname}`.\n\
         #[test]{async_attr}\n\
         fn {test_name}() {{\n\
         {body}}}\n",
        fname = sig.name,
        test_name = test_name,
        async_attr = async_attr,
        body = body,
    )
}

fn generate_error_test(sig: &FnSignature, mod_name: &str) -> String {
    let test_name = format!("test_{}_error_cases", sig.name);
    let async_attr = if sig.is_async {
        "\n#[tokio::test]"
    } else {
        ""
    };
    let await_kw = if sig.is_async { ".await" } else { "" };

    let mut body = String::from("    // Arrange: invalid / failing inputs\n");
    for param in &sig.params {
        let fail_val = fail_value(&param.type_str);
        body.push_str(&format!(
            "    let {} = {}; // intentionally bad for {}\n",
            param.name, fail_val, param.type_str
        ));
    }

    let call = build_call(sig, &generate_fail_values(&sig.params));
    body.push_str(&format!(
        "    // Act & Assert — should return Err\n    let result = {}{};\n    assert!(result.is_err(), \"expected Err for invalid input\");\n",
        call, await_kw,
    ));

    format!(
        "/// Error-case test for `{mod_name}::{fname}`.\n\
         #[test]{async_attr}\n\
         fn {test_name}() {{\n\
         {body}}}\n",
        fname = sig.name,
        test_name = test_name,
        async_attr = async_attr,
        body = body,
    )
}

// ── Parsing ──────────────────────────────────────────────────────────

fn parse_fns(source: &str) -> Vec<FnSignature> {
    let mut results = Vec::new();
    let mut in_impl_block = false;
    let mut impl_depth = 0usize;
    let mut in_test_mod = false;
    let mut test_mod_depth = 0usize;

    for (line_num, raw_line) in source.lines().enumerate() {
        let line = raw_line.trim();

        if line.starts_with("impl ") || line.starts_with("impl<") {
            in_impl_block = true;
            impl_depth += line.chars().filter(|&c| c == '{').count();
            if !line.contains('{') {
                impl_depth = impl_depth.saturating_add(1);
            }
            continue;
        }
        if in_impl_block {
            impl_depth += line.chars().filter(|&c| c == '{').count();
            impl_depth = impl_depth.saturating_sub(line.chars().filter(|&c| c == '}').count());
            if impl_depth == 0 {
                in_impl_block = false;
            }
            continue;
        }

        if line.starts_with("#[cfg(test)]") || line.starts_with("mod tests") {
            in_test_mod = true;
            test_mod_depth += 1;
            continue;
        }
        if in_test_mod {
            test_mod_depth += line.chars().filter(|&c| c == '{').count();
            test_mod_depth =
                test_mod_depth.saturating_sub(line.chars().filter(|&c| c == '}').count());
            if test_mod_depth == 0 {
                in_test_mod = false;
            }
            continue;
        }

        if !line.starts_with("pub fn ") && !line.starts_with("pub async fn ") {
            continue;
        }

        let is_async = line.starts_with("pub async fn ");
        let after_pub = if is_async {
            line.strip_prefix("pub async fn ")
        } else {
            line.strip_prefix("pub fn ")
        };

        let Some(rest) = after_pub else {
            continue;
        };

        let name_end = rest
            .find(|c: char| c == '(' || c == '<')
            .unwrap_or(rest.len());
        let name = rest[..name_end].trim().to_string();
        if name.is_empty() {
            continue;
        }

        let params_start = rest.find('(');
        let params_end = rest.rfind(')');

        let params = if let (Some(start), Some(end)) = (params_start, params_end) {
            parse_params(&rest[start + 1..end])
        } else {
            Vec::new()
        };

        let after_params = params_end.map(|ep| &rest[ep + 1..]).unwrap_or("");
        let return_type = after_params
            .trim()
            .strip_prefix("->")
            .map(|s| s.trim().trim_end_matches('{').trim().to_string());

        let returns_result = return_type
            .as_deref()
            .map(|rt| rt.starts_with("Result<"))
            .unwrap_or(false);
        let returns_option = return_type
            .as_deref()
            .map(|rt| rt.starts_with("Option<"))
            .unwrap_or(false);

        results.push(FnSignature {
            name,
            signature: line.to_string(),
            params,
            return_type,
            returns_result,
            returns_option,
            is_async,
            line: line_num + 1,
        });
    }

    results
}

fn parse_params(params_str: &str) -> Vec<ParamInfo> {
    if params_str.trim().is_empty() {
        return Vec::new();
    }

    let mut params = Vec::new();
    let mut depth = 0usize;
    let mut current = String::new();

    for ch in params_str.chars() {
        match ch {
            '<' | '(' | '[' | '{' => {
                depth += 1;
                current.push(ch);
            }
            '>' | ')' | ']' | '}' => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            ',' if depth == 0 => {
                if let Some(pi) = parse_single_param(current.trim()) {
                    params.push(pi);
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    if let Some(pi) = parse_single_param(current.trim()) {
        params.push(pi);
    }

    params
}

fn parse_single_param(s: &str) -> Option<ParamInfo> {
    let s = s.trim();
    if s.is_empty() || s == "self" || s == "&self" || s == "&mut self" {
        return None;
    }
    let colon = s.find(':')?;
    let name = s[..colon].trim().to_string();
    let type_str = s[colon + 1..].trim().to_string();
    if name.is_empty() || type_str.is_empty() {
        return None;
    }
    Some(ParamInfo { name, type_str })
}

// ── Value generation ─────────────────────────────────────────────────

fn generate_dummy_values(params: &[ParamInfo]) -> Vec<(String, String)> {
    params
        .iter()
        .map(|p| (p.name.clone(), dummy_value(&p.type_str)))
        .collect()
}

fn generate_edge_values(params: &[ParamInfo]) -> Vec<(String, String)> {
    params
        .iter()
        .map(|p| (p.name.clone(), edge_value(&p.type_str)))
        .collect()
}

fn generate_fail_values(params: &[ParamInfo]) -> Vec<(String, String)> {
    params
        .iter()
        .map(|p| (p.name.clone(), fail_value(&p.type_str)))
        .collect()
}

fn build_call(sig: &FnSignature, values: &[(String, String)]) -> String {
    let args = values
        .iter()
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>()
        .join(", ");
    format!("{}({args})", sig.name)
}

fn dummy_value(type_str: &str) -> String {
    let t = type_str.trim();
    match t {
        "bool" => "true".into(),
        "u8" | "u16" | "u32" | "u64" | "u128" | "usize" => "42".into(),
        "i8" | "i16" | "i32" | "i64" | "i128" | "isize" => "-1".into(),
        "f32" | "f64" => "3.14".into(),
        "String" => "\"hello\".into()".into(),
        "&str" => "\"hello\"".into(),
        "char" => "'x'".into(),
        "()" => "()".into(),
        "&[u8]" => "b\"hello\"".into(),
        t if t.starts_with("Vec<") => "vec![]".into(),
        t if t.starts_with("Option<") => {
            let inner = t.trim_start_matches("Option<").trim_end_matches('>').trim();
            if inner == "String" || inner == "&str" {
                "Some(\"test\".to_string())".into()
            } else {
                format!("Some({})", dummy_value(inner))
            }
        }
        t if t.starts_with("HashMap<") => "std::collections::HashMap::new()".into(),
        t if t.starts_with("HashSet<") => "std::collections::HashSet::new()".into(),
        t if t.starts_with("PathBuf") => "std::path::PathBuf::from(\".\")".into(),
        t if t.starts_with("&Path") => "std::path::Path::new(\".\")".into(),
        t if t.starts_with('&') => dummy_value(&t[1..].trim()),
        _ => format!("Default::default() /* TODO: fill {t} */"),
    }
}

fn edge_value(type_str: &str) -> String {
    let t = type_str.trim();
    match t {
        "bool" => "false".into(),
        "u8" | "u16" | "u32" | "u64" | "u128" | "usize" => "0".into(),
        "i8" | "i16" | "i32" | "i64" | "i128" | "isize" => "0".into(),
        "f32" | "f64" => "0.0".into(),
        "String" => "String::new()".into(),
        "&str" => "\"\"".into(),
        "char" => "'\\0'".into(),
        "()" => "()".into(),
        "&[u8]" => "&[]".into(),
        t if t.starts_with("Vec<") => "vec![]".into(),
        t if t.starts_with("Option<") => "None".into(),
        t if t.starts_with("HashMap<") => "std::collections::HashMap::new()".into(),
        t if t.starts_with("HashSet<") => "std::collections::HashSet::new()".into(),
        t if t.starts_with("PathBuf") => "std::path::PathBuf::new()".into(),
        t if t.starts_with("&Path") => "std::path::Path::new(\"\")".into(),
        t if t.starts_with('&') => edge_value(&t[1..].trim()),
        _ => "Default::default()".into(),
    }
}

fn fail_value(type_str: &str) -> String {
    let t = type_str.trim();
    match t {
        "bool" => "false".into(),
        "u8" => "u8::MAX".into(),
        "u16" => "u16::MAX".into(),
        "u32" => "u32::MAX".into(),
        "u64" => "u64::MAX".into(),
        "u128" => "u128::MAX".into(),
        "usize" => "usize::MAX".into(),
        "i8" => "i8::MIN".into(),
        "i16" => "i16::MIN".into(),
        "i32" => "i32::MIN".into(),
        "i64" => "i64::MIN".into(),
        "i128" => "i128::MIN".into(),
        "isize" => "isize::MIN".into(),
        "f32" => "f32::NAN".into(),
        "f64" => "f64::NAN".into(),
        "String" => "\"\".to_string()".into(),
        "&str" => "\"\"".into(),
        "char" => "'\\u{FFFF}'".into(),
        "()" => "()".into(),
        "&[u8]" => "&[]".into(),
        t if t.starts_with("Vec<") => "vec![]".into(),
        t if t.starts_with("Option<") => "None".into(),
        t if t.starts_with("HashMap<") => "std::collections::HashMap::new()".into(),
        t if t.starts_with("HashSet<") => "std::collections::HashSet::new()".into(),
        t if t.starts_with("PathBuf") => "std::path::PathBuf::new()".into(),
        t if t.starts_with("&Path") => "std::path::Path::new(\"/nonexistent/path\")".into(),
        t if t.starts_with('&') => fail_value(&t[1..].trim()),
        _ => "Default::default()".into(),
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_pub_fn() {
        let src = "pub fn add(a: u32, b: u32) -> u32 { a + b }";
        let sigs = parse_fns(src);
        assert_eq!(sigs.len(), 1);
        assert_eq!(sigs[0].name, "add");
        assert_eq!(sigs[0].params.len(), 2);
        assert_eq!(sigs[0].params[0].name, "a");
        assert_eq!(sigs[0].params[0].type_str, "u32");
        assert_eq!(sigs[0].params[1].name, "b");
        assert_eq!(sigs[0].params[1].type_str, "u32");
        assert_eq!(sigs[0].return_type.as_deref(), Some("u32"));
        assert!(!sigs[0].returns_result);
        assert!(!sigs[0].is_async);
    }

    #[test]
    fn test_parse_pub_fn_with_result() {
        let src = "pub fn divide(a: u32, b: u32) -> Result<u32, String> { Ok(a / b) }";
        let sigs = parse_fns(src);
        assert_eq!(sigs.len(), 1);
        assert!(sigs[0].returns_result);
        assert_eq!(sigs[0].name, "divide");
    }

    #[test]
    fn test_parse_pub_async_fn() {
        let src =
            "pub async fn fetch_data(url: String) -> Result<String, reqwest::Error> { Ok(String::new()) }";
        let sigs = parse_fns(src);
        assert_eq!(sigs.len(), 1);
        assert!(sigs[0].is_async);
        assert!(sigs[0].returns_result);
        assert_eq!(sigs[0].name, "fetch_data");
    }

    #[test]
    fn test_parse_skips_private_fn() {
        let src =
            "fn helper(x: i32) -> i32 { x * 2 }\npub fn public_fn(x: i32) -> i32 { helper(x) }";
        let sigs = parse_fns(src);
        assert_eq!(sigs.len(), 1);
        assert_eq!(sigs[0].name, "public_fn");
    }

    #[test]
    fn test_parse_no_pub_fns() {
        let sigs = parse_fns("fn a() {}\nfn b() {}");
        assert!(sigs.is_empty());
    }

    #[test]
    fn test_parse_empty_params() {
        let sigs = parse_fns("pub fn greet() -> String { \"hello\".into() }");
        assert_eq!(sigs.len(), 1);
        assert_eq!(sigs[0].name, "greet");
        assert!(sigs[0].params.is_empty());
    }

    #[test]
    fn test_parse_with_generics() {
        let src = "pub fn first<T>(items: Vec<T>) -> Option<T> { items.into_iter().next() }";
        let sigs = parse_fns(src);
        assert_eq!(sigs.len(), 1);
        assert_eq!(sigs[0].name, "first");
        assert!(sigs[0].returns_option);
    }

    #[test]
    fn test_generate_tests_for_result_fn() {
        let sig = FnSignature {
            name: "divide".into(),
            signature: "pub fn divide(a: u32, b: u32) -> Result<u32, String>".into(),
            params: vec![
                ParamInfo {
                    name: "a".into(),
                    type_str: "u32".into(),
                },
                ParamInfo {
                    name: "b".into(),
                    type_str: "u32".into(),
                },
            ],
            return_type: Some("Result<u32, String>".into()),
            returns_result: true,
            returns_option: false,
            is_async: false,
            line: 1,
        };

        let happy = generate_happy_path_test(&sig, "test_mod");
        assert!(happy.contains("#[test]"));
        assert!(happy.contains("test_divide_happy_path"));
        assert!(happy.contains("let a = 42"));
        assert!(happy.contains("let b = 42"));

        let edge = generate_edge_case_test(&sig, "test_mod");
        assert!(edge.contains("test_divide_edge_cases"));
        assert!(edge.contains("let a = 0"));

        let err = generate_error_test(&sig, "test_mod");
        assert!(err.contains("test_divide_error_cases"));
        assert!(err.contains("is_err()"));
    }

    #[test]
    fn test_generate_tests_non_existent_file() {
        let tests = generate_tests("/nonexistent/file.rs");
        assert!(tests.is_empty());
    }

    #[test]
    fn test_dummy_value_for_common_types() {
        assert_eq!(dummy_value("u32"), "42");
        assert_eq!(dummy_value("String"), "\"hello\".into()");
        assert_eq!(dummy_value("&str"), "\"hello\"");
        assert_eq!(dummy_value("bool"), "true");
        assert_eq!(dummy_value("Vec<String>"), "vec![]");
        assert_eq!(dummy_value("Option<String>"), "Some(\"test\".to_string())");
        assert_eq!(dummy_value("PathBuf"), "std::path::PathBuf::from(\".\")");
    }

    #[test]
    fn test_edge_value_for_common_types() {
        assert_eq!(edge_value("u32"), "0");
        assert_eq!(edge_value("String"), "String::new()");
        assert_eq!(edge_value("&str"), "\"\"");
        assert_eq!(edge_value("bool"), "false");
        assert_eq!(edge_value("Vec<String>"), "vec![]");
        assert_eq!(edge_value("Option<String>"), "None");
    }

    #[test]
    fn test_fail_value_for_common_types() {
        assert_eq!(fail_value("u32"), "u32::MAX");
        assert_eq!(fail_value("i32"), "i32::MIN");
        assert_eq!(fail_value("f64"), "f64::NAN");
        assert_eq!(fail_value("String"), "\"\".to_string()");
    }

    #[test]
    fn test_parse_skips_impl_block_methods() {
        let src = "pub fn helper() -> u32 { 42 }\n\nimpl MyStruct {\n    pub fn method(&self) -> u32 { 42 }\n}\n\npub fn another() -> u32 { 7 }";
        let sigs = parse_fns(src);
        assert_eq!(sigs.len(), 2, "only top-level pub fns, not impl methods");
        assert_eq!(sigs[0].name, "helper");
        assert_eq!(sigs[1].name, "another");
    }
}
