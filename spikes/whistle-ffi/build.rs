//! Links the binary-only `libneedle.a` (Cactus Compute, Apache-2.0) and a *static* libc++.
//!
//! - `NEEDLE_DIR`: folder with `libneedle.a` for the current target.
//! - `LIBCXX_DIR`: folder with `libc++.a` + `libc++abi.a` (+ `libunwind.a` on gnullvm).
use std::env;

fn main() {
    let home = env::var("HOME").unwrap_or_default();
    let cache = format!("{home}/.cache/pando-assistant-spikes");
    let target = env::var("TARGET").unwrap_or_default();
    let windows = target.contains("windows");
    let needle = env::var("NEEDLE_DIR").unwrap_or_else(|_| {
        let plat = if windows {
            "windows-x86_64"
        } else {
            "linux-x86_64"
        };
        format!("{cache}/needle/{plat}")
    });
    println!("cargo:rerun-if-env-changed=NEEDLE_DIR");
    println!("cargo:rerun-if-env-changed=LIBCXX_DIR");
    println!("cargo:rustc-link-search=native={needle}");
    println!("cargo:rustc-link-lib=static=needle");
    if windows {
        // llvm-mingw sysroot: libc++ / libc++abi / libunwind are the GNU-ABI ones.
        if let Ok(dir) = env::var("LIBCXX_DIR") {
            println!("cargo:rustc-link-search=native={dir}");
        }
        println!("cargo:rustc-link-lib=static=c++");
        println!("cargo:rustc-link-lib=static=c++abi");
        println!("cargo:rustc-link-lib=static=unwind");
    } else {
        let libcxx = env::var("LIBCXX_DIR")
            .unwrap_or_else(|_| format!("{cache}/llvm/root/usr/lib/x86_64-linux-gnu"));
        println!("cargo:rustc-link-search=native={libcxx}");
        println!("cargo:rustc-link-lib=static=c++");
        println!("cargo:rustc-link-lib=static=c++abi");
        for l in ["pthread", "m", "dl"] {
            println!("cargo:rustc-link-lib=dylib={l}");
        }
    }
}
