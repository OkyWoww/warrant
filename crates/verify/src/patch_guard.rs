//! # Patch Anti-Bypass & Signature Validator (SPEC.md Section 5.3 & Task T11)
//!
//! Enforces strict static/AST security invariants on all remediation patch candidates:
//! 1. Signature Tampering Prevention: Public items (functions, structs, methods) cannot be deleted or have signatures altered.
//! 2. Unconditional Abort / Stubbing Prevention: Functions cannot unconditionally abort (`todo!()`, `unimplemented!()`, `panic!()`, `process::exit`).
//! 3. Trivial Empty / Cfg Bypass Prevention: Code cannot be wrapped in `#[cfg(not(miri))]` or `#[cfg(false)]`, or trivially stubbed with `Default::default()`.
//! 4. Harness Modification Prevention: The proof harness (`mod warrant_proof`) must remain 100% byte-for-byte identical.

use quote::ToTokens;
use std::collections::HashMap;
use thiserror::Error;
use syn::{
    parse_file, Attribute, Expr, File, ImplItem, Item, ReturnType, Stmt, Visibility,
};

/// Errors returned by the Patch Anti-Bypass Guard
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PatchGuardError {
    #[error("Syntax error parsing patch AST: {0}")]
    SyntaxError(String),

    #[error("Signature tampering detected: {0}")]
    SignatureTampering(String),

    #[error("Unconditional abort or stubbing detected: {0}")]
    UnconditionalAbort(String),

    #[error("Forbidden cfg bypass attribute detected: {0}")]
    CfgBypass(String),

    #[error("Trivial empty or default stubbing detected: {0}")]
    TrivialStubbing(String),

    #[error("Proof harness was modified: {0}")]
    HarnessModified(String),
}

/// Canonical representation of a public item's signature
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicSignature {
    pub item_name: String,
    pub inputs_tokens: String,
    pub output_tokens: String,
    pub is_unsafe: bool,
    pub is_async: bool,
}

/// Validates that a candidate patch satisfies all anti-bypass and signature invariants
pub fn validate_patch(original_code: &str, patched_code: &str) -> Result<(), PatchGuardError> {
    let orig_ast = parse_file(original_code)
        .map_err(|e| PatchGuardError::SyntaxError(format!("Original code AST invalid: {}", e)))?;
    let patch_ast = parse_file(patched_code)
        .map_err(|e| PatchGuardError::SyntaxError(format!("Patched code AST invalid: {}", e)))?;

    // 1. Check for forbidden #[cfg(...)] bypasses across the entire patched AST
    check_cfg_bypasses(&patch_ast)?;

    // 2. Check for unconditional aborts and stubbing in all patched function bodies
    check_unconditional_aborts(&patch_ast)?;

    // 3. Extract public signatures and verify no signature tampering
    let orig_sigs = extract_public_signatures(&orig_ast);
    let patch_sigs = extract_public_signatures(&patch_ast);

    for (symbol_id, orig_sig) in &orig_sigs {
        match patch_sigs.get(symbol_id) {
            None => {
                return Err(PatchGuardError::SignatureTampering(format!(
                    "Public symbol '{}' was removed or made non-public in the patch",
                    orig_sig.item_name
                )));
            }
            Some(patch_sig) => {
                if orig_sig.inputs_tokens != patch_sig.inputs_tokens {
                    return Err(PatchGuardError::SignatureTampering(format!(
                        "Public symbol '{}' argument types modified: expected '{}', got '{}'",
                        orig_sig.item_name, orig_sig.inputs_tokens, patch_sig.inputs_tokens
                    )));
                }
                if orig_sig.output_tokens != patch_sig.output_tokens {
                    return Err(PatchGuardError::SignatureTampering(format!(
                        "Public symbol '{}' return type modified: expected '{}', got '{}'",
                        orig_sig.item_name, orig_sig.output_tokens, patch_sig.output_tokens
                    )));
                }
                if orig_sig.is_unsafe != patch_sig.is_unsafe {
                    return Err(PatchGuardError::SignatureTampering(format!(
                        "Public symbol '{}' unsafe qualifier changed from {} to {}",
                        orig_sig.item_name, orig_sig.is_unsafe, patch_sig.is_unsafe
                    )));
                }
            }
        }
    }

    // 4. Check for trivial default/empty stubbing on functions that originally had substantive bodies
    check_trivial_stubbing(&orig_ast, &patch_ast)?;

    Ok(())
}

/// Verify that the harness text appears byte-for-byte at the END of the source file that is
/// about to be executed (SPEC.md 5.2 step 1: appended at the end; 5.3: harness unchanged).
pub fn verify_harness_in_source(source: &str, harness: &str) -> Result<(), PatchGuardError> {
    let clean_harness = harness.trim_start_matches('\u{feff}').trim();
    if source.trim_end().ends_with(clean_harness) {
        Ok(())
    } else {
        Err(PatchGuardError::HarnessModified(
            "Harness text is not present verbatim at the end of the verified source file".to_string(),
        ))
    }
}

/// Verify that the proof harness was not altered in any way
pub fn verify_harness_unchanged(original_harness: &str, tested_harness: &str) -> Result<(), PatchGuardError> {
    if original_harness.trim_start_matches('\u{feff}').trim()
        != tested_harness.trim_start_matches('\u{feff}').trim()
    {
        return Err(PatchGuardError::HarnessModified(
            "Proof harness code was modified during patch verification loop".to_string(),
        ));
    }
    Ok(())
}

/// Invariant 1: Extract all public function and method signatures
fn extract_public_signatures(file: &File) -> HashMap<String, PublicSignature> {
    let mut sigs = HashMap::new();

    for item in &file.items {
        match item {
            Item::Fn(item_fn) => {
                if matches!(item_fn.vis, Visibility::Public(_)) {
                    let name = item_fn.sig.ident.to_string();
                    let key = format!("fn:{}", name);
                    sigs.insert(
                        key,
                        PublicSignature {
                            item_name: name,
                            inputs_tokens: normalize_tokens(&item_fn.sig.inputs),
                            output_tokens: normalize_tokens(&item_fn.sig.output),
                            is_unsafe: item_fn.sig.unsafety.is_some(),
                            is_async: item_fn.sig.asyncness.is_some(),
                        },
                    );
                }
            }
            Item::Impl(item_impl) => {
                let self_ty = normalize_tokens(&item_impl.self_ty);
                for impl_item in &item_impl.items {
                    if let ImplItem::Fn(impl_fn) = impl_item {
                        if matches!(impl_fn.vis, Visibility::Public(_)) {
                            let fn_name = impl_fn.sig.ident.to_string();
                            let key = format!("method:{}::{}", self_ty, fn_name);
                            sigs.insert(
                                key,
                                PublicSignature {
                                    item_name: format!("{}::{}", self_ty, fn_name),
                                    inputs_tokens: normalize_tokens(&impl_fn.sig.inputs),
                                    output_tokens: normalize_tokens(&impl_fn.sig.output),
                                    is_unsafe: impl_fn.sig.unsafety.is_some(),
                                    is_async: impl_fn.sig.asyncness.is_some(),
                                },
                            );
                        }
                    }
                }
            }
            Item::Struct(item_struct) => {
                if matches!(item_struct.vis, Visibility::Public(_)) {
                    let name = item_struct.ident.to_string();
                    let key = format!("struct:{}", name);
                    // Only public fields form part of the public API signature;
                    // private internal fields are encapsulation details.
                    let pub_fields: Vec<String> = item_struct
                        .fields
                        .iter()
                        .filter(|f| matches!(f.vis, Visibility::Public(_)))
                        .map(|f| {
                            let fname = f.ident.as_ref().map(|id| id.to_string()).unwrap_or_default();
                            let fty = normalize_tokens(&f.ty);
                            format!("{}: {}", fname, fty)
                        })
                        .collect();
                    sigs.insert(
                        key,
                        PublicSignature {
                            item_name: name,
                            inputs_tokens: pub_fields.join(", "),
                            output_tokens: String::new(),
                            is_unsafe: false,
                            is_async: false,
                        },
                    );
                }
            }
            Item::Enum(item_enum) => {
                if matches!(item_enum.vis, Visibility::Public(_)) {
                    let name = item_enum.ident.to_string();
                    let key = format!("enum:{}", name);
                    sigs.insert(
                        key,
                        PublicSignature {
                            item_name: name,
                            inputs_tokens: normalize_tokens(&item_enum.variants),
                            output_tokens: String::new(),
                            is_unsafe: false,
                            is_async: false,
                        },
                    );
                }
            }
            _ => {}
        }
    }

    sigs
}

/// Invariant 2: Check for unconditional aborts (`todo!()`, `unimplemented!()`, `panic!()`, `process::exit`)
fn check_unconditional_aborts(file: &File) -> Result<(), PatchGuardError> {
    for item in &file.items {
        match item {
            Item::Fn(item_fn) => {
                validate_stmts_not_aborting(&item_fn.sig.ident.to_string(), &item_fn.block.stmts)?;
            }
            Item::Impl(item_impl) => {
                let self_ty = normalize_tokens(&item_impl.self_ty);
                for impl_item in &item_impl.items {
                    if let ImplItem::Fn(impl_fn) = impl_item {
                        let name = format!("{}::{}", self_ty, impl_fn.sig.ident);
                        validate_stmts_not_aborting(&name, &impl_fn.block.stmts)?;
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn validate_stmts_not_aborting(fn_name: &str, stmts: &[Stmt]) -> Result<(), PatchGuardError> {
    if stmts.is_empty() {
        return Ok(());
    }

    // Inspect the first statement in the function body
    let first = &stmts[0];
    if let Some(reason) = is_abort_statement(first) {
        return Err(PatchGuardError::UnconditionalAbort(format!(
            "Function '{}' unconditionally aborts execution via {}",
            fn_name, reason
        )));
    }

    Ok(())
}

fn is_abort_statement(stmt: &Stmt) -> Option<String> {
    match stmt {
        Stmt::Macro(stmt_macro) => {
            let path_str = normalize_tokens(&stmt_macro.mac.path);
            if matches_abort_macro(&path_str) {
                return Some(format!("macro `{}!`", path_str));
            }
        }
        Stmt::Expr(expr, _) => match expr {
            Expr::Macro(expr_macro) => {
                let path_str = normalize_tokens(&expr_macro.mac.path);
                if matches_abort_macro(&path_str) {
                    return Some(format!("macro `{}!`", path_str));
                }
            }
            Expr::Call(call) => {
                let path_str = normalize_tokens(&call.func);
                if path_str.contains("exit") || path_str.contains("abort") {
                    return Some(format!("function call `{}`", path_str));
                }
            }
            _ => {}
        },
        _ => {}
    }
    None
}

fn matches_abort_macro(macro_path: &str) -> bool {
    let base = macro_path.rsplit("::").next().unwrap_or(macro_path).trim();
    matches!(base, "todo" | "unimplemented" | "unreachable" | "panic")
}

/// Invariant 3: Check for `#[cfg(not(miri))]` or `#[cfg(false)]`
fn check_cfg_bypasses(file: &File) -> Result<(), PatchGuardError> {
    for attr in &file.attrs {
        check_single_attr(attr)?;
    }

    for item in &file.items {
        match item {
            Item::Fn(f) => {
                for attr in &f.attrs {
                    check_single_attr(attr)?;
                }
            }
            Item::Impl(i) => {
                for attr in &i.attrs {
                    check_single_attr(attr)?;
                }
                for impl_item in &i.items {
                    if let ImplItem::Fn(m) = impl_item {
                        for attr in &m.attrs {
                            check_single_attr(attr)?;
                        }
                    }
                }
            }
            Item::Struct(s) => {
                for attr in &s.attrs {
                    check_single_attr(attr)?;
                }
            }
            Item::Enum(e) => {
                for attr in &e.attrs {
                    check_single_attr(attr)?;
                }
            }
            _ => {}
        }
    }

    Ok(())
}

fn check_single_attr(attr: &Attribute) -> Result<(), PatchGuardError> {
    if attr.path().is_ident("cfg") {
        let meta_str = normalize_tokens(&attr.meta);
        if meta_str.contains("not(miri)")
            || meta_str.contains("not (miri)")
            || meta_str.contains("false")
            || meta_str.contains("not(all())")
            || meta_str.contains("not (all ())")
        {
            return Err(PatchGuardError::CfgBypass(format!(
                "Attribute '#[cfg({})]' attempts to bypass Miri execution",
                meta_str
            )));
        }
    }
    Ok(())
}

/// Invariant 4: Check for trivial empty / `Default::default()` stubbing
fn check_trivial_stubbing(orig_file: &File, patch_file: &File) -> Result<(), PatchGuardError> {
    let mut orig_body_lengths = HashMap::new();

    for item in &orig_file.items {
        if let Item::Impl(item_impl) = item {
            let self_ty = normalize_tokens(&item_impl.self_ty);
            for impl_item in &item_impl.items {
                if let ImplItem::Fn(impl_fn) = impl_item {
                    let key = format!("{}::{}", self_ty, impl_fn.sig.ident);
                    orig_body_lengths.insert(key, (impl_fn.block.stmts.len(), impl_fn.sig.output.clone()));
                }
            }
        }
    }

    for item in &patch_file.items {
        if let Item::Impl(item_impl) = item {
            let self_ty = normalize_tokens(&item_impl.self_ty);
            for impl_item in &item_impl.items {
                if let ImplItem::Fn(impl_fn) = impl_item {
                    let key = format!("{}::{}", self_ty, impl_fn.sig.ident);
                    if let Some((orig_len, orig_output)) = orig_body_lengths.get(&key) {
                        if *orig_len > 1 && !matches!(orig_output, ReturnType::Default) {
                            if impl_fn.block.stmts.is_empty() {
                                return Err(PatchGuardError::TrivialStubbing(format!(
                                    "Method '{}' was emptied into trivial empty stub",
                                    key
                                )));
                            }
                            if impl_fn.block.stmts.len() == 1 {
                                let stmt_tokens = normalize_tokens(&impl_fn.block.stmts[0]);
                                if stmt_tokens.contains("Default :: default")
                                    || stmt_tokens.contains("Default::default")
                                {
                                    return Err(PatchGuardError::TrivialStubbing(format!(
                                        "Method '{}' was trivially stubbed with Default::default()",
                                        key
                                    )));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

fn normalize_tokens<T: ToTokens>(item: &T) -> String {
    let raw = item.to_token_stream().to_string();
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_ORIGINAL: &str = r#"
pub struct SliceBuffer {
    data: Vec<u8>,
    cursor: usize,
}

impl SliceBuffer {
    pub fn new() -> Self {
        Self { data: Vec::new(), cursor: 0 }
    }

    pub fn read_at_offset(&self, offset: usize) -> Option<u8> {
        let idx = self.cursor.checked_add(offset)?;
        Some(unsafe { *self.data.get_unchecked(idx) })
    }
}
"#;

    const VALID_PATCHED: &str = r#"
pub struct SliceBuffer {
    data: Vec<u8>,
    cursor: usize,
}

impl SliceBuffer {
    pub fn new() -> Self {
        Self { data: Vec::new(), cursor: 0 }
    }

    pub fn read_at_offset(&self, offset: usize) -> Option<u8> {
        let idx = self.cursor.checked_add(offset)?;
        if idx < self.data.len() {
            Some(unsafe { *self.data.get_unchecked(idx) })
        } else {
            None
        }
    }
}
"#;

    #[test]
    fn test_valid_patch_passes_guard() {
        let res = validate_patch(VALID_ORIGINAL, VALID_PATCHED);
        assert!(res.is_ok(), "Sound patch with preserved signatures must pass: {:?}", res);
    }

    #[test]
    fn test_adversarial_signature_tampering_removed_fn() {
        // Bad patch: deleted read_at_offset
        let bad_patch = r#"
pub struct SliceBuffer {
    data: Vec<u8>,
    cursor: usize,
}

impl SliceBuffer {
    pub fn new() -> Self {
        Self { data: Vec::new(), cursor: 0 }
    }
}
"#;
        let res = validate_patch(VALID_ORIGINAL, bad_patch);
        assert!(res.is_err(), "Must reject patch that removes public method");
        assert!(matches!(res.unwrap_err(), PatchGuardError::SignatureTampering(_)));
    }

    #[test]
    fn test_adversarial_signature_tampering_altered_params() {
        // Bad patch: changed offset from usize to u32
        let bad_patch = r#"
pub struct SliceBuffer {
    data: Vec<u8>,
    cursor: usize,
}

impl SliceBuffer {
    pub fn new() -> Self {
        Self { data: Vec::new(), cursor: 0 }
    }

    pub fn read_at_offset(&self, offset: u32) -> Option<u8> {
        None
    }
}
"#;
        let res = validate_patch(VALID_ORIGINAL, bad_patch);
        assert!(res.is_err(), "Must reject patch that changes argument type");
        assert!(matches!(res.unwrap_err(), PatchGuardError::SignatureTampering(_)));
    }

    #[test]
    fn test_adversarial_cfg_not_miri_bypass() {
        // Bad patch: wraps method in #[cfg(not(miri))]
        let bad_patch = r#"
pub struct SliceBuffer {
    data: Vec<u8>,
    cursor: usize,
}

impl SliceBuffer {
    pub fn new() -> Self {
        Self { data: Vec::new(), cursor: 0 }
    }

    #[cfg(not(miri))]
    pub fn read_at_offset(&self, offset: usize) -> Option<u8> {
        let idx = self.cursor.checked_add(offset)?;
        Some(unsafe { *self.data.get_unchecked(idx) })
    }
}
"#;
        let res = validate_patch(VALID_ORIGINAL, bad_patch);
        assert!(res.is_err(), "Must reject patch with #[cfg(not(miri))]");
        assert!(matches!(res.unwrap_err(), PatchGuardError::CfgBypass(_)));
    }

    #[test]
    fn test_adversarial_cfg_false_bypass() {
        // Bad patch: uses #[cfg(false)]
        let bad_patch = r#"
#[cfg(false)]
pub struct SliceBuffer {
    data: Vec<u8>,
}
"#;
        let res = validate_patch(VALID_ORIGINAL, bad_patch);
        assert!(res.is_err(), "Must reject patch with #[cfg(false)]");
        assert!(matches!(res.unwrap_err(), PatchGuardError::CfgBypass(_)));
    }

    #[test]
    fn test_adversarial_todo_macro_stubbing() {
        // Bad patch: replaces logic with todo!()
        let bad_patch = r#"
pub struct SliceBuffer {
    data: Vec<u8>,
    cursor: usize,
}

impl SliceBuffer {
    pub fn new() -> Self {
        Self { data: Vec::new(), cursor: 0 }
    }

    pub fn read_at_offset(&self, offset: usize) -> Option<u8> {
        todo!("unimplemented")
    }
}
"#;
        let res = validate_patch(VALID_ORIGINAL, bad_patch);
        assert!(res.is_err(), "Must reject patch with todo!()");
        assert!(matches!(res.unwrap_err(), PatchGuardError::UnconditionalAbort(_)));
    }

    #[test]
    fn test_adversarial_panic_macro_stubbing() {
        // Bad patch: unconditional panic
        let bad_patch = r#"
pub struct SliceBuffer {
    data: Vec<u8>,
    cursor: usize,
}

impl SliceBuffer {
    pub fn new() -> Self {
        Self { data: Vec::new(), cursor: 0 }
    }

    pub fn read_at_offset(&self, offset: usize) -> Option<u8> {
        panic!("stubbed")
    }
}
"#;
        let res = validate_patch(VALID_ORIGINAL, bad_patch);
        assert!(res.is_err(), "Must reject patch with panic!()");
        assert!(matches!(res.unwrap_err(), PatchGuardError::UnconditionalAbort(_)));
    }

    #[test]
    fn test_adversarial_trivial_default_stubbing() {
        // Bad patch: replaces substantive body with Default::default()
        let bad_patch = r#"
pub struct SliceBuffer {
    data: Vec<u8>,
    cursor: usize,
}

impl SliceBuffer {
    pub fn new() -> Self {
        Self { data: Vec::new(), cursor: 0 }
    }

    pub fn read_at_offset(&self, offset: usize) -> Option<u8> {
        Default::default()
    }
}
"#;
        let res = validate_patch(VALID_ORIGINAL, bad_patch);
        assert!(res.is_err(), "Must reject patch trivially stubbed with Default::default()");
        assert!(matches!(res.unwrap_err(), PatchGuardError::TrivialStubbing(_)));
    }

    #[test]
    fn test_harness_tampering_detected() {
        let orig_harness = "#![forbid(unsafe_code)]\nextern crate std;\n#[test]\nfn proof_r1() {}";
        let tampered_harness = "#![forbid(unsafe_code)]\nextern crate std;\n#[test]\nfn proof_tampered() {}";

        let res = verify_harness_unchanged(orig_harness, tampered_harness);
        assert!(res.is_err(), "Must detect modified proof harness");
        assert!(matches!(res.unwrap_err(), PatchGuardError::HarnessModified(_)));
    }
}
