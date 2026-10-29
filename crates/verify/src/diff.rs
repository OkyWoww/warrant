//! # Unified Diff Generator
//!
//! Generates clean, standard unified diffs for Project WARRANT remediation patches.
//! Follows standard POSIX / Git unified diff format without requiring external dependencies.

use std::fmt::Write;

/// Represents an edit operation in a unified diff
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffOp<'a> {
    Equal(&'a str),
    Delete(&'a str),
    Insert(&'a str),
}

/// Compute line-level LCS (Longest Common Subsequence) diff between two texts
pub fn compute_diff<'a>(original: &'a str, patched: &'a str) -> Vec<DiffOp<'a>> {
    let orig_lines: Vec<&'a str> = original.lines().collect();
    let patch_lines: Vec<&'a str> = patched.lines().collect();

    let n = orig_lines.len();
    let m = patch_lines.len();

    // DP table for LCS
    let mut dp = vec![vec![0usize; m + 1]; n + 1];

    for i in 0..n {
        for j in 0..m {
            if orig_lines[i] == patch_lines[j] {
                dp[i + 1][j + 1] = dp[i][j] + 1;
            } else {
                dp[i + 1][j + 1] = dp[i + 1][j].max(dp[i][j + 1]);
            }
        }
    }

    // Backtrack to recover diff operations
    let mut ops = Vec::new();
    let mut i = n;
    let mut j = m;

    while i > 0 || j > 0 {
        if i > 0 && j > 0 && orig_lines[i - 1] == patch_lines[j - 1] {
            ops.push(DiffOp::Equal(orig_lines[i - 1]));
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || dp[i][j - 1] >= dp[i - 1][j]) {
            ops.push(DiffOp::Insert(patch_lines[j - 1]));
            j -= 1;
        } else if i > 0 && (j == 0 || dp[i][j - 1] < dp[i - 1][j]) {
            ops.push(DiffOp::Delete(orig_lines[i - 1]));
            i -= 1;
        }
    }

    ops.reverse();
    ops
}

/// Generate formatted unified diff string (`--- a/path\n+++ b/path\n...`)
pub fn generate_unified_diff(original: &str, patched: &str, file_name: &str) -> String {
    let ops = compute_diff(original, patched);
    let mut diff = String::new();

    let _ = writeln!(diff, "--- a/{}", file_name);
    let _ = writeln!(diff, "+++ b/{}", file_name);

    let has_changes = ops.iter().any(|op| !matches!(op, DiffOp::Equal(_)));
    if !has_changes {
        let _ = writeln!(diff, "@@ -1,0 +1,0 @@ (no changes)");
        return diff;
    }

    let orig_count = original.lines().count();
    let patch_count = patched.lines().count();
    let _ = writeln!(diff, "@@ -1,{} +1,{} @@", orig_count, patch_count);

    for op in ops {
        match op {
            DiffOp::Equal(line) => {
                let _ = writeln!(diff, " {}", line);
            }
            DiffOp::Delete(line) => {
                let _ = writeln!(diff, "-{}", line);
            }
            DiffOp::Insert(line) => {
                let _ = writeln!(diff, "+{}", line);
            }
        }
    }

    diff
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_identical_strings() {
        let code = "fn foo() -> i32 {\n    42\n}\n";
        let diff = generate_unified_diff(code, code, "src/lib.rs");
        assert!(diff.contains("(no changes)"));
    }

    #[test]
    fn test_diff_insertion_and_deletion() {
        let orig = "pub fn test() {\n    let x = 1;\n}\n";
        let patched = "pub fn test() {\n    let x = 2;\n    let y = 3;\n}\n";
        let diff = generate_unified_diff(orig, patched, "src/lib.rs");

        assert!(diff.contains("--- a/src/lib.rs"));
        assert!(diff.contains("+++ b/src/lib.rs"));
        assert!(diff.contains("-    let x = 1;"));
        assert!(diff.contains("+    let x = 2;"));
        assert!(diff.contains("+    let y = 3;"));
    }
}

