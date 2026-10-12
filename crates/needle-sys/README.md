# needle-sys

Hand-written FFI bindings for `needle.h` (needle3 commit `93e88ede`) and the `build.rs` that links
`libneedle.a` plus a static libc++ (the engine is built with clang + libc++). No bindgen/libclang needed.

## Getting the engine

`libneedle.a` is binary-only and never committed. Fetch it (SHA-256 verified) with

```bash
scripts/fetch-needle.sh [platform] [--model]     # -> vendor/needle/<platform>/{libneedle.a,needle.h}
```

`build.rs` looks for the engine in `$NEEDLE_DIR`, then `vendor/needle/<platform>/`. If it is missing, the build
fails with instructions; set `NEEDLE_ALLOW_MISSING=1` to only warn (e.g. `cargo check` without the engine; linking
a binary then fails).

## libc++

Linux needs static `libc++.a` + `libc++abi.a` (LLVM >= 21, e.g. apt.llvm.org `libc++-21-dev libc++abi-21-dev`).
Searched in `$LIBCXX_DIR`, `vendor/needle/<platform>/lib/`, `/usr/lib/llvm-*/lib`, `/usr/lib/<triple>`, `/usr/local/lib`.
macOS links the system `c++`. Windows (`x86_64-pc-windows-gnullvm`) needs the llvm-mingw `libc++.a`, `libc++abi.a`,
`libunwind.a` in `$LIBCXX_DIR`.

Do not link this crate into a process that also loads sherpa-onnx/libstdc++ code (ADR 0007, spike S3).
