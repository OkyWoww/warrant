//! # warrant-extract
//!
//! AST extraction engine for project WARRANT.
//! Parses Rust crates and files via `syn` to identify all unsafe code candidates
//! strictly for rules R1 (out-of-bounds / precondition violations) and R2 (Send/Sync data races),
//! preserving item spans, context, visibility, and safety invariants without compiler type resolvers.
//!
//! Strictly adheres to SPEC.md Section 4, Section 5.1, and Section 10 (Task T4).

use quote::ToTokens;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use thiserror::Error;
use walkdir::WalkDir;

pub use warrant_llm::{Candidate, CandidateKind, ExtractionTarget, SpecCandidate};

#[derive(Debug, Error)]
pub enum ExtractError {
    #[error("Failed to read file {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Failed to parse Rust syntax in {path}: {source}")]
    Parse {
        path: PathBuf,
        source: syn::Error,
    },
}

/// Visitor that inspects AST expressions to determine if they match rule R1 patterns:
/// - `get_unchecked`, `get_unchecked_mut`
/// - `from_raw_parts`, `from_raw_parts_mut`
/// - Pointer offset / arithmetic (`offset`, `add`, `sub`, `wrapping_*`, `byte_*`, `offset_from`)
/// - Pointer dereference unary `*` (`*ptr`)
/// - Pointer read/write functions (`std::ptr::read`, `copy`, etc.)
#[derive(Default, Debug)]
struct R1Detector {
    is_r1: bool,
    matched_pattern: Option<String>,
}

impl<'ast> Visit<'ast> for R1Detector {
    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        let method = node.method.to_string();
        match method.as_str() {
            "get_unchecked"
            | "get_unchecked_mut"
            | "offset"
            | "add"
            | "sub"
            | "wrapping_offset"
            | "wrapping_add"
            | "wrapping_sub"
            | "byte_add"
            | "byte_sub"
            | "byte_offset_from"
            | "offset_from"
            | "read"
            | "read_volatile"
            | "read_unaligned"
            | "write"
            | "write_volatile"
            | "write_unaligned" => {
                self.is_r1 = true;
                self.matched_pattern = Some(format!("method:{}", method));
                return;
            }
            _ => {}
        }
        visit::visit_expr_method_call(self, node);
    }

    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if let syn::Expr::Path(ref expr_path) = *node.func {
            if let Some(segment) = expr_path.path.segments.last() {
                let ident = segment.ident.to_string();
                match ident.as_str() {
                    "from_raw_parts"
                    | "from_raw_parts_mut"
                    | "get_unchecked"
                    | "get_unchecked_mut"
                    | "copy"
                    | "copy_nonoverlapping"
                    | "read"
                    | "write"
                    | "read_volatile"
                    | "write_volatile"
                    | "read_unaligned"
                    | "write_unaligned" => {
                        self.is_r1 = true;
                        self.matched_pattern = Some(format!("fn:{}", ident));
                        return;
                    }
                    _ => {}
                }
            }
        }
        visit::visit_expr_call(self, node);
    }

    fn visit_expr_unary(&mut self, node: &'ast syn::ExprUnary) {
        if let syn::UnOp::Deref(_) = node.op {
            // Unary dereference *ptr
            self.is_r1 = true;
            self.matched_pattern = Some("deref:*".to_string());
            return;
        }
        visit::visit_expr_unary(self, node);
    }
}

/// Visitor that walks the syn AST and collects unsafe candidates for R1 and R2.
struct UnsafeVisitor<'a> {
    file_path: String,
    crate_name: &'a str,
    source_lines: &'a [&'a str],
    current_impl_type: Option<String>,
    current_enclosing_item: Option<String>,
    current_visibility: String,
    struct_visibilities: HashMap<String, String>,
    candidate_counter: usize,
    pub candidates: Vec<Candidate>,
}

impl<'a> UnsafeVisitor<'a> {
    fn new(file_path: &str, crate_name: &'a str, source_lines: &'a [&'a str]) -> Self {
        // Normalize path with forward slashes
        let normalized_path = file_path.replace('\\', "/");
        Self {
            file_path: normalized_path,
            crate_name,
            source_lines,
            current_impl_type: None,
            current_enclosing_item: None,
            current_visibility: "pub".to_string(),
            struct_visibilities: HashMap::new(),
            candidate_counter: 1,
            candidates: Vec::new(),
        }
    }

    fn next_candidate_id(&mut self) -> String {
        let id = format!("c-{:04}", self.candidate_counter);
        self.candidate_counter += 1;
        id
    }

    fn calculate_span_lines<T: Spanned>(&self, node: &T) -> (usize, usize) {
        let span = node.span();
        let start = span.start().line.max(1);
        let end = span.end().line.max(start);
        (start, end)
    }

    fn extract_safety_comment(&self, start_line: usize) -> Option<String> {
        if start_line <= 1 {
            return None;
        }
        let mut comments = Vec::new();
        let mut idx = start_line - 1;
        while idx > 0 {
            if let Some(line_str) = self.source_lines.get(idx - 1) {
                let trimmed = line_str.trim();
                if trimmed.starts_with("//") || trimmed.starts_with("///") {
                    comments.push(trimmed.to_string());
                    idx -= 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        if comments.is_empty() {
            None
        } else {
            comments.reverse();
            Some(comments.join("\n"))
        }
    }

    fn map_visibility(vis: &syn::Visibility) -> String {
        match vis {
            syn::Visibility::Public(_) => "pub".to_string(),
            syn::Visibility::Restricted(_) => "restricted".to_string(),
            syn::Visibility::Inherited => "private".to_string(),
        }
    }

    fn has_cfg_test(attrs: &[syn::Attribute]) -> bool {
        attrs.iter().any(|attr| {
            if attr.path().is_ident("test") {
                return true;
            }
            if attr.path().is_ident("cfg") {
                let s = attr.meta.to_token_stream().to_string();
                if s.contains("test") {
                    return true;
                }
            }
            false
        })
    }
}

impl<'ast, 'a> Visit<'ast> for UnsafeVisitor<'a> {
    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        // Skip test modules and verification harnesses (#[cfg(test)], mod tests, mod warrant_proof)
        if node.ident == "warrant_proof" || node.ident == "tests" || Self::has_cfg_test(&node.attrs) {
            return;
        }
        visit::visit_item_mod(self, node);
    }

    fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
        let name = node.ident.to_string();
        let vis = Self::map_visibility(&node.vis);
        self.struct_visibilities.insert(name, vis);
        visit::visit_item_struct(self, node);
    }

    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        let name = node.ident.to_string();
        let vis = Self::map_visibility(&node.vis);
        self.struct_visibilities.insert(name, vis);
        visit::visit_item_enum(self, node);
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        let old_impl_type = self.current_impl_type.clone();
        let old_enclosing = self.current_enclosing_item.clone();
        let old_visibility = self.current_visibility.clone();

        let self_ty_str = node.self_ty.to_token_stream().to_string().replace(' ', "");

        // R2 Check: unsafe impl Send / unsafe impl Sync
        if node.unsafety.is_some() {
            if let Some((_, ref trait_path, _)) = node.trait_ {
                if let Some(last_seg) = trait_path.segments.last() {
                    let trait_name = last_seg.ident.to_string();
                    if trait_name == "Sync" || trait_name == "Send" {
                        let (start_line, end_line) = self.calculate_span_lines(node);
                        let id = self.next_candidate_id();
                        let safety_comment = self.extract_safety_comment(start_line);
                        let code_snippet = node.to_token_stream().to_string();
                        let visibility = self
                            .struct_visibilities
                            .get(&self_ty_str)
                            .cloned()
                            .unwrap_or_else(|| "pub".to_string());

                        self.candidates.push(Candidate {
                            id,
                            crate_name: self.crate_name.to_string(),
                            file_path: self.file_path.clone(),
                            start_line,
                            end_line,
                            kind: CandidateKind::UnsafeImpl,
                            code_snippet,
                            enclosing_item: Some(self_ty_str.clone()),
                            safety_comment,
                            trait_name: Some(trait_name),
                            rule: Some("R2".to_string()),
                            visibility: Some(visibility),
                        });
                    }
                }
            }
        }

        self.current_impl_type = Some(self_ty_str.clone());
        self.current_enclosing_item = Some(self_ty_str);

        // Recurse into impl items (methods)
        visit::visit_item_impl(self, node);

        self.current_impl_type = old_impl_type;
        self.current_enclosing_item = old_enclosing;
        self.current_visibility = old_visibility;
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        if Self::has_cfg_test(&node.attrs) {
            return;
        }

        let old_enclosing = self.current_enclosing_item.clone();
        let old_vis = self.current_visibility.clone();

        let fn_name = node.sig.ident.to_string();
        let enclosing = match &self.current_impl_type {
            Some(impl_ty) => format!("{}::{}", impl_ty, fn_name),
            None => fn_name.clone(),
        };

        self.current_enclosing_item = Some(enclosing.clone());
        self.current_visibility = Self::map_visibility(&node.vis);

        // If the method itself is marked `unsafe fn`
        if node.sig.unsafety.is_some() {
            let mut detector = R1Detector::default();
            detector.visit_impl_item_fn(node);

            if detector.is_r1 {
                let (start_line, end_line) = self.calculate_span_lines(node);
                let id = self.next_candidate_id();
                let safety_comment = self.extract_safety_comment(start_line);
                let code_snippet = node.to_token_stream().to_string();

                self.candidates.push(Candidate {
                    id,
                    crate_name: self.crate_name.to_string(),
                    file_path: self.file_path.clone(),
                    start_line,
                    end_line,
                    kind: CandidateKind::UnsafeFn,
                    code_snippet,
                    enclosing_item: Some(enclosing),
                    safety_comment,
                    trait_name: None,
                    rule: Some("R1".to_string()),
                    visibility: Some(self.current_visibility.clone()),
                });

                // Do not double-count inner unsafe blocks if entire fn is candidate
                self.current_enclosing_item = old_enclosing;
                self.current_visibility = old_vis;
                return;
            }
        }

        visit::visit_impl_item_fn(self, node);

        self.current_enclosing_item = old_enclosing;
        self.current_visibility = old_vis;
    }

    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        if Self::has_cfg_test(&node.attrs) {
            return;
        }

        let old_enclosing = self.current_enclosing_item.clone();
        let old_vis = self.current_visibility.clone();

        let fn_name = node.sig.ident.to_string();
        self.current_enclosing_item = Some(fn_name.clone());
        self.current_visibility = Self::map_visibility(&node.vis);

        // If standalone function is marked `unsafe fn`
        if node.sig.unsafety.is_some() {
            let mut detector = R1Detector::default();
            detector.visit_item_fn(node);

            if detector.is_r1 {
                let (start_line, end_line) = self.calculate_span_lines(node);
                let id = self.next_candidate_id();
                let safety_comment = self.extract_safety_comment(start_line);
                let code_snippet = node.to_token_stream().to_string();

                self.candidates.push(Candidate {
                    id,
                    crate_name: self.crate_name.to_string(),
                    file_path: self.file_path.clone(),
                    start_line,
                    end_line,
                    kind: CandidateKind::UnsafeFn,
                    code_snippet,
                    enclosing_item: Some(fn_name),
                    safety_comment,
                    trait_name: None,
                    rule: Some("R1".to_string()),
                    visibility: Some(self.current_visibility.clone()),
                });

                self.current_enclosing_item = old_enclosing;
                self.current_visibility = old_vis;
                return;
            }
        }

        visit::visit_item_fn(self, node);

        self.current_enclosing_item = old_enclosing;
        self.current_visibility = old_vis;
    }

    fn visit_expr_unsafe(&mut self, node: &'ast syn::ExprUnsafe) {
        // Inspect if this unsafe block matches R1 (unchecked / pointer ops / dereference)
        let mut detector = R1Detector::default();
        detector.visit_expr_unsafe(node);

        if detector.is_r1 {
            let (start_line, end_line) = self.calculate_span_lines(node);
            let id = self.next_candidate_id();
            let safety_comment = self.extract_safety_comment(start_line);
            let code_snippet = node.to_token_stream().to_string();

            self.candidates.push(Candidate {
                id,
                crate_name: self.crate_name.to_string(),
                file_path: self.file_path.clone(),
                start_line,
                end_line,
                kind: CandidateKind::UnsafeBlock,
                code_snippet,
                enclosing_item: self.current_enclosing_item.clone(),
                safety_comment,
                trait_name: None,
                rule: Some("R1".to_string()),
                visibility: Some(self.current_visibility.clone()),
            });
        }

        visit::visit_expr_unsafe(self, node);
    }
}

/// Extract all unsafe candidates for rules R1 and R2 from a single Rust source string.
pub fn extract_candidates_from_source(
    source: &str,
    file_path: &str,
    crate_name: &str,
) -> Result<Vec<Candidate>, ExtractError> {
    let syntax = syn::parse_file(source).map_err(|source| ExtractError::Parse {
        path: PathBuf::from(file_path),
        source,
    })?;

    let lines: Vec<&str> = source.lines().collect();
    let mut visitor = UnsafeVisitor::new(file_path, crate_name, &lines);
    visitor.visit_file(&syntax);

    Ok(visitor.candidates)
}

/// Extract all unsafe candidates for rules R1 and R2 from a single Rust file on disk.
pub fn extract_candidates_from_file(
    file_path: &Path,
    crate_name: &str,
) -> Result<Vec<Candidate>, ExtractError> {
    let source = fs::read_to_string(file_path).map_err(|source| ExtractError::Io {
        path: file_path.to_path_buf(),
        source,
    })?;

    let path_str = file_path.to_string_lossy();
    extract_candidates_from_source(&source, &path_str, crate_name)
}

/// Recursively traverse a directory and extract all unsafe candidates across all `.rs` files.
pub fn extract_candidates_from_directory(
    dir_path: &Path,
    crate_name: &str,
) -> Result<Vec<Candidate>, ExtractError> {
    let mut all_candidates = Vec::new();

    for entry in WalkDir::new(dir_path)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "rs"))
    {
        let path = entry.path();
        let file_candidates = extract_candidates_from_file(path, crate_name)?;
        all_candidates.extend(file_candidates);
    }

    Ok(all_candidates)
}

/// Extract candidates formatted as canonical SpecCandidate (SPEC.md Section 4) from source string.
pub fn extract_spec_candidates_from_source(
    source: &str,
    file_path: &str,
    crate_name: &str,
) -> Result<Vec<SpecCandidate>, ExtractError> {
    let candidates = extract_candidates_from_source(source, file_path, crate_name)?;
    Ok(candidates.iter().map(|c| c.to_spec_candidate()).collect())
}

/// Extract candidates formatted as canonical SpecCandidate (SPEC.md Section 4) from a file.
pub fn extract_spec_candidates_from_file(
    file_path: &Path,
    crate_name: &str,
) -> Result<Vec<SpecCandidate>, ExtractError> {
    let candidates = extract_candidates_from_file(file_path, crate_name)?;
    Ok(candidates.iter().map(|c| c.to_spec_candidate()).collect())
}

/// Extract candidates formatted as canonical SpecCandidate (SPEC.md Section 4) from a directory.
pub fn extract_spec_candidates_from_directory(
    dir_path: &Path,
    crate_name: &str,
) -> Result<Vec<SpecCandidate>, ExtractError> {
    let candidates = extract_candidates_from_directory(dir_path, crate_name)?;
    Ok(candidates.iter().map(|c| c.to_spec_candidate()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_r1_slice_reference_fixture() {
        let fixture_path = Path::new("../../bench/pairs/r1_reference.rs");
        let alt_path = Path::new("bench/pairs/r1_reference.rs");
        let path = if fixture_path.exists() {
            fixture_path
        } else {
            alt_path
        };

        let candidates =
            extract_candidates_from_file(path, "target_crate").expect("must extract candidates");

        assert_eq!(candidates.len(), 1, "Must find exactly 1 R1 candidate in r1_reference.rs");
        let cand = &candidates[0];

        assert_eq!(cand.id, "c-0001");
        assert_eq!(cand.rule.as_deref(), Some("R1"));
        assert_eq!(cand.kind, CandidateKind::UnsafeBlock);
        assert_eq!(cand.enclosing_item.as_deref(), Some("FastBuffer::read_unchecked_elem"));
        assert_eq!(cand.visibility.as_deref(), Some("pub"));
        assert_eq!(cand.start_line, 21);
        assert_eq!(cand.end_line, 21);
        assert_eq!(cand.line_span(), [21, 21]);

        // Validate SPEC schema output
        let spec = cand.to_spec_candidate();
        assert_eq!(spec.id, "c-0001");
        assert_eq!(spec.rule, "R1");
        assert_eq!(spec.line_span, [21, 21]);
        assert_eq!(spec.enclosing_item, "FastBuffer::read_unchecked_elem");
        assert_eq!(spec.visibility, "pub");
        assert_eq!(spec.kind, CandidateKind::UnsafeBlock);

        let spec_json = serde_json::to_string(&spec).expect("must serialize");
        let reparsed = SpecCandidate::parse_from_json(&spec_json).expect("must validate spec schema");
        assert_eq!(reparsed.id, "c-0001");
    }

    #[test]
    fn test_extract_r1_ptr_deref_fixture() {
        let fixture_path = Path::new("../../bench/pairs/r1_ptr_deref.rs");
        let alt_path = Path::new("bench/pairs/r1_ptr_deref.rs");
        let path = if fixture_path.exists() {
            fixture_path
        } else {
            alt_path
        };

        let candidates =
            extract_candidates_from_file(path, "target_crate").expect("must extract candidates");

        assert_eq!(candidates.len(), 1, "Must find exactly 1 R1 candidate in r1_ptr_deref.rs");
        let cand = &candidates[0];

        assert_eq!(cand.id, "c-0001");
        assert_eq!(cand.rule.as_deref(), Some("R1"));
        assert_eq!(cand.kind, CandidateKind::UnsafeBlock);
        assert_eq!(cand.enclosing_item.as_deref(), Some("RawBuffer::read_offset"));
        assert_eq!(cand.visibility.as_deref(), Some("pub"));
        assert_eq!(cand.start_line, 25);
        assert_eq!(cand.end_line, 25);
        assert_eq!(cand.line_span(), [25, 25]);
    }

    #[test]
    fn test_extract_r2_reference_fixture() {
        let fixture_path = Path::new("../../bench/pairs/r2_reference.rs");
        let alt_path = Path::new("bench/pairs/r2_reference.rs");
        let path = if fixture_path.exists() {
            fixture_path
        } else {
            alt_path
        };

        let candidates =
            extract_candidates_from_file(path, "target_crate").expect("must extract candidates");

        assert_eq!(candidates.len(), 1, "Must find exactly 1 R2 candidate in r2_reference.rs");
        let cand = &candidates[0];

        assert_eq!(cand.id, "c-0001");
        assert_eq!(cand.rule.as_deref(), Some("R2"));
        assert_eq!(cand.kind, CandidateKind::UnsafeImpl);
        assert_eq!(cand.enclosing_item.as_deref(), Some("SharedCounter"));
        assert_eq!(cand.trait_name.as_deref(), Some("Sync"));
        assert_eq!(cand.visibility.as_deref(), Some("pub"));
        assert_eq!(cand.start_line, 33);
        assert_eq!(cand.end_line, 33);
        assert_eq!(cand.line_span(), [33, 33]);
    }

    #[test]
    fn test_negative_control_fixture_finds_zero_candidates() {
        let fixture_path = Path::new("../../bench/pairs/safe_control.rs");
        let alt_path = Path::new("bench/pairs/safe_control.rs");
        let path = if fixture_path.exists() {
            fixture_path
        } else {
            alt_path
        };

        let candidates =
            extract_candidates_from_file(path, "target_crate").expect("must extract cleanly");

        assert_eq!(
            candidates.len(),
            0,
            "Safe control fixture must NOT yield any R1 or R2 candidates"
        );
    }

    #[test]
    fn test_extract_from_source_contract_serialization() {
        let code = r#"
            pub struct FrameReader {
                buffer: Vec<u8>,
            }

            impl FrameReader {
                pub fn next_frame(&self) -> u8 {
                    // SAFETY: precondition unchecked
                    unsafe { *self.buffer.get_unchecked(14) }
                }
            }
        "#;

        let candidates =
            extract_candidates_from_source(code, "src/frame.rs", "test_crate").unwrap();
        assert_eq!(candidates.len(), 1);
        let cand = &candidates[0];
        assert_eq!(cand.enclosing_item.as_deref(), Some("FrameReader::next_frame"));
        assert_eq!(cand.visibility.as_deref(), Some("pub"));
        assert_eq!(cand.rule.as_deref(), Some("R1"));
        assert_eq!(cand.kind, CandidateKind::UnsafeBlock);

        let target = cand.to_extraction_target();
        assert_eq!(target.target_symbol, "FrameReader::next_frame");
        assert_eq!(target.rule, "R1");

        let spec = cand.to_spec_candidate();
        let json_value = serde_json::to_value(&spec).unwrap();
        assert_eq!(json_value["id"], "c-0001");
        assert_eq!(json_value["rule"], "R1");
        assert_eq!(json_value["file"], "src/frame.rs");
        assert_eq!(json_value["enclosing_item"], "FrameReader::next_frame");
        assert_eq!(json_value["visibility"], "pub");
        assert_eq!(json_value["kind"], "unsafe_block");
        assert_eq!(json_value["line_span"], serde_json::json!([cand.start_line, cand.end_line]));
    }
}
