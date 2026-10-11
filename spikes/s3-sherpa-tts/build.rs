// Spike S3 link experiments (feature `needle` only):
// - builds a tiny g++ (libstdc++) exception shim, `eh_shim.cpp`;
// - links libneedle.a (clang + libc++ 21) and static LLVM libc++ next to
//   sherpa-onnx (which brings libstdc++ + bundled ORT + espeak-ng).
// Env knobs:
//   NEEDLE_DIR, LIBCXX_DIR   where libneedle.a / libc++.a, libc++abi.a live
//   S3_ONLY_SHIM=1           baseline: shim only, no libneedle / libc++
//   S3_NO_LIBCXXABI=1        link libc++.a but take the C++ ABI from libstdc++
//   S3_NEEDLE_PRIVATE=1      NEEDLE_DIR holds a symbol-private libneedle.a (libc++ inside)
use std::process::Command;

fn main() {
    println!("cargo::rustc-check-cfg=cfg(s3_only_shim)");
    for v in [
        "NEEDLE_DIR",
        "LIBCXX_DIR",
        "S3_ONLY_SHIM",
        "S3_NO_LIBCXXABI",
        "S3_NEEDLE_PRIVATE",
    ] {
        println!("cargo:rerun-if-env-changed={v}");
    }
    println!("cargo:rerun-if-changed=src/eh_shim.cpp");
    if std::env::var_os("CARGO_FEATURE_NEEDLE").is_none() {
        return;
    }
    let out = std::env::var("OUT_DIR").unwrap();
    let ok = Command::new("g++")
        .args(["-O2", "-fPIC", "-c", "src/eh_shim.cpp", "-o"])
        .arg(format!("{out}/eh_shim.o"))
        .status()
        .unwrap()
        .success();
    assert!(ok, "g++ failed");
    let ok = Command::new("ar")
        .args([
            "rcs",
            &format!("{out}/libeh_shim.a"),
            &format!("{out}/eh_shim.o"),
        ])
        .status()
        .unwrap()
        .success();
    assert!(ok, "ar failed");
    println!("cargo:rustc-link-search=native={out}");
    println!("cargo:rustc-link-lib=static=eh_shim");

    if std::env::var_os("S3_ONLY_SHIM").is_some() {
        println!("cargo:rustc-cfg=s3_only_shim");
        println!("cargo:rustc-link-lib=dylib=stdc++");
        return;
    }
    let needle = std::env::var("NEEDLE_DIR").expect("NEEDLE_DIR");
    let libcxx = std::env::var("LIBCXX_DIR").expect("LIBCXX_DIR");
    println!("cargo:rustc-link-search=native={needle}");
    println!("cargo:rustc-link-search=native={libcxx}");
    println!("cargo:rustc-link-lib=static=needle");
    if std::env::var_os("S3_NEEDLE_PRIVATE").is_some() {
        // libneedle.a pre-linked with libc++ and all non-needle_* symbols localised
        // (ld -r + objcopy --keep-global-symbols), see the S3 report.
        println!("cargo:rustc-link-lib=dylib=stdc++");
        return;
    }
    println!("cargo:rustc-link-lib=static=c++");
    if std::env::var_os("S3_NO_LIBCXXABI").is_none() {
        println!("cargo:rustc-link-lib=static=c++abi");
    }
    // ehtest does not use sherpa-onnx, so request libstdc++ explicitly.
    println!("cargo:rustc-link-lib=dylib=stdc++");
}
