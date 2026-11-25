//! Link the compiler runtime of the C toolchain on macOS.
//!
//! The ONNX runtime objects call `___isPlatformVersionAtLeast`, which the Rust
//! compiler runtime does not keep when thin LTO is on (the release profile), so a
//! release link fails with an undefined symbol. The C toolchain's runtime library
//! has it. This script finds that library and links it; it changes nothing on other
//! systems and fails soft: if the library is not found it only prints a warning.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    match runtime_library() {
        Some(library) => {
            if let (Some(dir), Some(name)) = (library.parent(), library.file_stem()) {
                let name = name.to_string_lossy();
                let name = name.strip_prefix("lib").unwrap_or(&name);
                println!("cargo:rustc-link-search=native={}", dir.display());
                println!("cargo:rustc-link-lib=static={name}");
            }
        }
        None => println!(
            "cargo:warning=libclang_rt.osx.a not found; a release link of the ONNX runtime may fail"
        ),
    }
}

/// The path of `libclang_rt.osx.a` as the C compiler reports it.
fn runtime_library() -> Option<PathBuf> {
    let output = Command::new("xcrun")
        .args(["clang", "--print-file-name=libclang_rt.osx.a"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    path.is_absolute().then_some(path).filter(|p| p.is_file())
}
