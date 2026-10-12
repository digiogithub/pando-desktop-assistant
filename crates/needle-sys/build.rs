//! Links the binary-only `libneedle.a` and a static libc++ (see README.md).
use std::env;
use std::path::{Path, PathBuf};

fn platform(target: &str) -> Option<&'static str> {
    Some(match () {
        _ if target.starts_with("x86_64") && target.contains("linux") => "linux-x86_64",
        _ if target.starts_with("aarch64") && target.contains("linux") => "linux-arm64",
        _ if target == "aarch64-apple-darwin" => "macos-arm64",
        _ if target.starts_with("x86_64") && target.contains("windows") => "windows-x86_64",
        _ => return None,
    })
}

fn has_libcxx(dir: &Path) -> bool {
    dir.join("libc++.a").is_file() && dir.join("libc++abi.a").is_file()
}

fn find_libcxx(vendor: &Path, triple_dir: &str) -> Option<PathBuf> {
    let mut cands: Vec<PathBuf> = Vec::new();
    if let Ok(d) = env::var("LIBCXX_DIR") {
        cands.push(d.into());
    }
    cands.push(vendor.join("lib"));
    if let Ok(rd) = std::fs::read_dir("/usr/lib") {
        let mut llvm: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with("llvm-"))
            })
            .map(|p| p.join("lib"))
            .collect();
        llvm.sort();
        llvm.reverse();
        cands.extend(llvm);
    }
    cands.push(PathBuf::from(format!("/usr/lib/{triple_dir}")));
    cands.push(PathBuf::from("/usr/local/lib"));
    cands.into_iter().find(|d| has_libcxx(d))
}

fn main() {
    println!("cargo:rerun-if-env-changed=NEEDLE_DIR");
    println!("cargo:rerun-if-env-changed=LIBCXX_DIR");
    println!("cargo:rerun-if-env-changed=NEEDLE_ALLOW_MISSING");
    println!("cargo:rerun-if-changed=build.rs");

    let target = env::var("TARGET").unwrap_or_default();
    let allow_missing = env::var("NEEDLE_ALLOW_MISSING").is_ok_and(|v| v == "1");
    let fail = |msg: String| {
        if allow_missing {
            println!("cargo:warning=needle-sys: {msg} (NEEDLE_ALLOW_MISSING=1, not linking)");
        } else {
            panic!("needle-sys: {msg}");
        }
    };

    let Some(plat) = platform(&target) else {
        fail(format!(
            "no Needle engine exists for target `{target}` (no macOS x86_64 build; use the Parakeet fallback)"
        ));
        return;
    };

    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let vendor = match env::var("NEEDLE_DIR") {
        Ok(d) => PathBuf::from(d),
        Err(_) => manifest.join("../../vendor/needle").join(plat),
    };
    println!(
        "cargo:rerun-if-changed={}",
        vendor.join("libneedle.a").display()
    );
    if !vendor.join("libneedle.a").is_file() {
        fail(format!(
            "libneedle.a not found in `{}`.\n  Run `scripts/fetch-needle.sh {plat}` from the repository root, or set NEEDLE_DIR to a folder containing libneedle.a",
            vendor.display()
        ));
        return;
    }
    println!("cargo:rustc-link-search=native={}", vendor.display());
    println!("cargo:rustc-link-lib=static=needle");

    if target.contains("apple") {
        println!("cargo:rustc-link-lib=dylib=c++");
        return;
    }
    let triple_dir = if target.starts_with("aarch64") {
        "aarch64-linux-gnu"
    } else {
        "x86_64-linux-gnu"
    };
    match find_libcxx(&vendor, triple_dir) {
        Some(dir) => println!("cargo:rustc-link-search=native={}", dir.display()),
        None => {
            fail(
                "static libc++.a/libc++abi.a not found; install libc++-21-dev libc++abi-21-dev (apt.llvm.org) or set LIBCXX_DIR"
                    .into(),
            );
            return;
        }
    }
    println!("cargo:rustc-link-lib=static=c++");
    println!("cargo:rustc-link-lib=static=c++abi");
    if target.contains("windows") {
        println!("cargo:rustc-link-lib=static=unwind");
    } else {
        for l in ["pthread", "m", "dl"] {
            println!("cargo:rustc-link-lib=dylib={l}");
        }
    }
}
