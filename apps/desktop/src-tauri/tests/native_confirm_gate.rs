//! ADR 0006: the owner's click is proven by `OwnerConfirmation`, and its only
//! constructor, `from_native_dialog`, may be called only from the shell's native
//! dialog module. The compiler cannot restrict a function to one module of
//! another crate, so this test scans the sources of the shell and of every crate.

// Test helpers outside #[test] functions may unwrap and expect.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

const CONSTRUCTOR: &str = "from_native_dialog";

/// The one shell file (relative to the workspace root) that may call it.
const SHELL_DIALOG_FILE: &str = "apps/desktop/src-tauri/src/native_confirm.rs";

/// The file that defines it. Its own unit tests build confirmations too.
const DEFINING_FILE: &str = "crates/model-store/src/lib.rs";

fn workspace_root() -> PathBuf {
    fs::canonicalize(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")).unwrap()
}

fn rust_files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files_under(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// `file:line: text` for every code line that names the constructor. Comment
/// lines are skipped: a comment cannot build a confirmation.
fn constructor_uses(source: &str, label: &str) -> Vec<String> {
    source
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim_start().starts_with("//"))
        .filter(|(_, line)| line.contains(CONSTRUCTOR))
        .map(|(i, line)| format!("{label}:{}: {}", i + 1, line.trim()))
        .collect()
}

#[test]
fn only_the_native_dialog_module_builds_an_owner_confirmation() {
    let root = workspace_root();
    let mut files = Vec::new();
    rust_files_under(&root.join("apps/desktop/src-tauri/src"), &mut files);
    for entry in fs::read_dir(root.join("crates")).unwrap() {
        rust_files_under(&entry.unwrap().path().join("src"), &mut files);
    }
    let labels: Vec<(PathBuf, String)> = files
        .into_iter()
        .map(|f| {
            let label = f
                .strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            (f, label)
        })
        .collect();
    let shell_files = labels
        .iter()
        .filter(|(_, l)| l.starts_with("apps/desktop/src-tauri/src/"))
        .count();
    assert!(
        shell_files >= 10,
        "the scan found {shell_files} shell files"
    );
    let mut problems = Vec::new();
    for (file, label) in &labels {
        if label == SHELL_DIALOG_FILE || label == DEFINING_FILE {
            continue;
        }
        problems.extend(constructor_uses(&fs::read_to_string(file).unwrap(), label));
    }
    assert!(
        problems.is_empty(),
        "`{CONSTRUCTOR}` called outside {SHELL_DIALOG_FILE}:\n{}",
        problems.join("\n")
    );
}

#[test]
fn the_constructor_scan_flags_a_call_and_skips_comments() {
    let call = "let c = OwnerConfirmation::from_native_dialog(&id);\n";
    assert_eq!(constructor_uses(call, "x.rs").len(), 1);
    let comment = "// from_native_dialog is called only by native_confirm\nfn f() {}\n";
    assert!(constructor_uses(comment, "x.rs").is_empty());
}

#[test]
fn the_native_dialog_module_exists_and_exposes_the_confirm_function() {
    let source = fs::read_to_string(workspace_root().join(SHELL_DIALOG_FILE)).unwrap();
    assert!(source.contains("pub fn confirm_download"));
}
