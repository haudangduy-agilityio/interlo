//! ADR 0007, the part that can be checked today: no workspace crate declares an
//! HTTP or socket client, and no workspace source opens a socket. Every outbound
//! call must go through the policy gate (guardrail 1), and a crate that can open
//! a connection by itself skips that gate.
//!
//! Scope: only workspace member crates. The optional dependencies of Tauri and
//! other third-party crates are out of scope.

// Test helpers outside #[test] functions may unwrap and expect.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{json, Value};

/// Crates that open network connections for their caller.
const BANNED_CRATES: &[&str] = &[
    "reqwest",
    "hyper",
    "ureq",
    "curl",
    "isahc",
    "attohttpc",
    "surf",
];

/// Socket types that no workspace source may name.
const BANNED_SOCKET_TYPES: &[&str] = &["TcpStream", "UdpSocket"];

/// Source files (path relative to the workspace root) that may name a socket
/// type. Empty on purpose: the policy-owned client of ADR 0007 does not exist yet.
const ALLOWED_SOCKET_FILES: &[&str] = &[];

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn cargo_metadata() -> Value {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        // `--no-deps`: only the workspace members and what they declare. The full
        // resolve would need every platform's crates in the offline cache.
        .args([
            "metadata",
            "--format-version",
            "1",
            "--offline",
            "--no-deps",
        ])
        .arg("--manifest-path")
        .arg(workspace_root().join("Cargo.toml"))
        .output()
        .expect("run cargo metadata");
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("cargo metadata prints JSON")
}

/// One line per forbidden dependency a workspace member declares (any kind:
/// normal, dev or build). `tokio` is forbidden only with a networking feature.
fn forbidden_dependencies(metadata: &Value) -> Vec<String> {
    let members: Vec<&str> = metadata["workspace_members"]
        .as_array()
        .expect("workspace_members")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let mut found = Vec::new();
    for package in metadata["packages"].as_array().expect("packages") {
        if !members.contains(&package["id"].as_str().unwrap_or_default()) {
            continue;
        }
        let owner = package["name"].as_str().unwrap_or("?");
        for dep in package["dependencies"].as_array().into_iter().flatten() {
            let name = dep["name"].as_str().unwrap_or_default();
            if BANNED_CRATES.contains(&name) {
                found.push(format!("{owner} depends on {name}"));
            }
            if name == "tokio" {
                let features = dep["features"].as_array().into_iter().flatten();
                for feature in features.filter_map(Value::as_str) {
                    if feature == "net" || feature == "full" {
                        found.push(format!("{owner} depends on tokio with feature {feature}"));
                    }
                }
            }
        }
    }
    found
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

/// `file:line: text` for every code line that names a banned socket type.
/// Comment lines are skipped.
fn socket_uses(source: &str, label: &str) -> Vec<String> {
    source
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim_start().starts_with("//"))
        .filter(|(_, line)| BANNED_SOCKET_TYPES.iter().any(|t| line.contains(t)))
        .map(|(i, line)| format!("{label}:{}: {}", i + 1, line.trim()))
        .collect()
}

fn source_roots() -> Vec<PathBuf> {
    let root = workspace_root();
    let mut roots = vec![root.join("apps/desktop/src-tauri/src")];
    for entry in fs::read_dir(root.join("crates")).unwrap() {
        roots.push(entry.unwrap().path().join("src"));
    }
    roots
}

/// Path of `file` relative to the workspace root, with `/` separators.
fn relative_label(file: &Path) -> String {
    let root = fs::canonicalize(workspace_root()).unwrap();
    let file = fs::canonicalize(file).unwrap();
    file.strip_prefix(root)
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/")
}

#[test]
fn no_workspace_crate_declares_an_http_or_socket_client() {
    let metadata = cargo_metadata();
    let members = metadata["workspace_members"].as_array().unwrap().len();
    assert!(members >= 13, "expected the whole workspace, got {members}");
    let found = forbidden_dependencies(&metadata);
    assert!(
        found.is_empty(),
        "network client dependency:\n{}",
        found.join("\n")
    );
}

#[test]
fn the_dependency_check_flags_a_client_and_tokio_net_and_accepts_tokio_rt() {
    let member = |dep: Value| {
        json!({
            "workspace_members": ["demo 0.1.0 (path+file:///demo)"],
            "packages": [{
                "id": "demo 0.1.0 (path+file:///demo)",
                "name": "demo",
                "dependencies": [dep],
            }],
        })
    };
    for client in BANNED_CRATES {
        let metadata = member(json!({ "name": client, "features": [] }));
        assert_eq!(forbidden_dependencies(&metadata).len(), 1, "{client}");
    }
    let net = member(json!({ "name": "tokio", "features": ["rt", "net"] }));
    assert_eq!(forbidden_dependencies(&net).len(), 1);
    let full = member(json!({ "name": "tokio", "features": ["full"] }));
    assert_eq!(forbidden_dependencies(&full).len(), 1);
    let rt = member(json!({ "name": "tokio", "features": ["rt", "sync"] }));
    assert!(forbidden_dependencies(&rt).is_empty());
    let outsider = json!({
        "workspace_members": [],
        "packages": [{
            "id": "other 1.0.0 (registry+x)",
            "name": "other",
            "dependencies": [{ "name": "reqwest", "features": [] }],
        }],
    });
    assert!(
        forbidden_dependencies(&outsider).is_empty(),
        "a crate outside the workspace is out of scope"
    );
}

#[test]
fn no_workspace_source_opens_a_socket() {
    let mut scanned = 0;
    let mut problems = Vec::new();
    for dir in source_roots() {
        let mut files = Vec::new();
        rust_files_under(&dir, &mut files);
        for file in files {
            scanned += 1;
            let label = relative_label(&file);
            if ALLOWED_SOCKET_FILES.contains(&label.as_str()) {
                continue;
            }
            problems.extend(socket_uses(&fs::read_to_string(&file).unwrap(), &label));
        }
    }
    assert!(scanned >= 15, "the scan found only {scanned} source files");
    assert!(
        problems.is_empty(),
        "socket type in source:\n{}",
        problems.join("\n")
    );
}

#[test]
fn the_socket_scan_flags_a_connect_and_skips_comments() {
    assert_eq!(
        socket_uses("let s = TcpStream::connect(addr)?;\n", "a.rs").len(),
        1
    );
    assert_eq!(
        socket_uses("use std::net::{TcpStream, UdpSocket};\n", "a.rs").len(),
        1
    );
    assert_eq!(
        socket_uses("let s = std::net::UdpSocket::bind(a)?;\n", "a.rs").len(),
        1
    );
    assert!(socket_uses("// no TcpStream here\nuse std::net::IpAddr;\n", "a.rs").is_empty());
}

#[test]
fn the_socket_allow_list_is_empty() {
    assert!(ALLOWED_SOCKET_FILES.is_empty());
}
