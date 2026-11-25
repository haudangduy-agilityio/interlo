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

/// `file:line: text` for every code line that holds `needle`. Comment lines are
/// skipped: a comment cannot build a confirmation or call a plugin.
fn uses_of(needle: &str, source: &str, label: &str) -> Vec<String> {
    source
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim_start().starts_with("//"))
        .filter(|(_, line)| line.contains(needle))
        .map(|(i, line)| format!("{label}:{}: {}", i + 1, line.trim()))
        .collect()
}

/// Every Rust source of the shell and of the crates: the file and its label
/// (the path relative to the workspace root).
fn workspace_sources(root: &Path) -> Vec<(PathBuf, String)> {
    let mut files = Vec::new();
    rust_files_under(&root.join("apps/desktop/src-tauri/src"), &mut files);
    for entry in fs::read_dir(root.join("crates")).unwrap() {
        rust_files_under(&entry.unwrap().path().join("src"), &mut files);
    }
    files
        .into_iter()
        .map(|f| {
            let label = f
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            (f, label)
        })
        .collect()
}

#[test]
fn only_the_native_dialog_module_builds_an_owner_confirmation() {
    let labels = workspace_sources(&workspace_root());
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
        problems.extend(uses_of(
            CONSTRUCTOR,
            &fs::read_to_string(file).unwrap(),
            label,
        ));
    }
    assert!(
        problems.is_empty(),
        "`{CONSTRUCTOR}` called outside {SHELL_DIALOG_FILE}:\n{}",
        problems.join("\n")
    );
}

#[test]
fn the_source_scan_flags_a_call_and_skips_comments() {
    let call = "let c = OwnerConfirmation::from_native_dialog(&id);\n";
    assert_eq!(uses_of(CONSTRUCTOR, call, "x.rs").len(), 1);
    let comment = "// from_native_dialog is called only by native_confirm\nfn f() {}\n";
    assert!(uses_of(CONSTRUCTOR, comment, "x.rs").is_empty());
    let plugin = "    .plugin(tauri_plugin_dialog::init())\n";
    assert_eq!(uses_of(PLUGIN_CRATE, plugin, "x.rs").len(), 1);
}

#[test]
fn the_native_dialog_module_exists_and_exposes_the_confirm_function() {
    let source = fs::read_to_string(workspace_root().join(SHELL_DIALOG_FILE)).unwrap();
    assert!(source.contains("pub fn confirm_download"));
}

/// The dialog plugin crate, as Rust code names it.
const PLUGIN_CRATE: &str = "tauri_plugin_dialog";

/// The only shell files that may name the dialog plugin crate: the module that
/// shows the dialog, and the builder that registers the plugin.
const PLUGIN_FILES: [&str; 2] = [SHELL_DIALOG_FILE, "apps/desktop/src-tauri/src/lib.rs"];

#[test]
fn only_the_dialog_module_and_the_registration_name_the_dialog_plugin() {
    let mut seen_in_allowed = Vec::new();
    let mut problems = Vec::new();
    for (file, label) in workspace_sources(&workspace_root()) {
        let uses = uses_of(PLUGIN_CRATE, &fs::read_to_string(&file).unwrap(), &label);
        if PLUGIN_FILES.contains(&label.as_str()) {
            if !uses.is_empty() {
                seen_in_allowed.push(label);
            }
        } else {
            problems.extend(uses);
        }
    }
    assert!(
        problems.is_empty(),
        "`{PLUGIN_CRATE}` named outside {PLUGIN_FILES:?}:\n{}",
        problems.join("\n")
    );
    seen_in_allowed.sort();
    let mut expected = PLUGIN_FILES.to_vec();
    expected.sort_unstable();
    assert_eq!(seen_in_allowed, expected, "both files use the plugin");
}

#[test]
fn only_the_dialog_module_builds_a_confirm_prompt() {
    // The dialog text and the model it confirms must come from one place:
    // `prompt_for` builds the prompt from the compiled-in table.
    let mut problems = Vec::new();
    for (file, label) in workspace_sources(&workspace_root()) {
        if label == SHELL_DIALOG_FILE {
            continue;
        }
        problems.extend(uses_of(
            "ConfirmPrompt {",
            &fs::read_to_string(&file).unwrap(),
            &label,
        ));
    }
    assert!(
        problems.is_empty(),
        "`ConfirmPrompt` built outside {SHELL_DIALOG_FILE}:\n{}",
        problems.join("\n")
    );
}
