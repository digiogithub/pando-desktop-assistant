//! Raw bindings for `needle.h` (needle3 commit `93e88ede867e4311c39151285e74df513e4b835b`).
//!
//! The engine keeps **process-global, non-thread-safe** state: call these functions from one thread
//! only. The safe wrapper lives in `pando-stt`. Strings are NUL-terminated; `out` buffers receive
//! NUL-terminated JSON.
#![allow(clippy::missing_safety_doc)]

use std::ffi::{c_char, c_float, c_int, c_uchar, c_ulonglong};

pub const NEEDLE_TEXT: c_int = 1;
pub const NEEDLE_SPEECH: c_int = 2;

unsafe extern "C" {
    /// Loads a `.cact` model from memory; negative on failure.
    pub fn needle_load(cact: *const c_uchar, n: c_ulonglong) -> c_int;
    /// Bit set of `NEEDLE_TEXT` / `NEEDLE_SPEECH` for loaded models.
    pub fn needle_models() -> c_int;
    pub fn needle_last_error() -> *const c_char;
    pub fn needle_init(
        system_prompt: *const c_char,
        tools_json: *const c_char,
        tool_index_path: *const c_char,
    ) -> c_int;
    pub fn needle_complete(
        input: *const c_char,
        pcm: *const c_float,
        samples: c_int,
        max_new_tokens: c_int,
        out: *mut c_char,
        out_capacity: c_int,
    ) -> c_int;
    pub fn needle_set_audio(
        language: *const c_char,
        keywords: *const c_char,
        word_timestamps: c_int,
    );
    pub fn needle_reset();
    pub fn needle_transcribe(
        pcm: *const c_float,
        samples: c_int,
        language: *const c_char,
        keywords: *const c_char,
        word_timestamps: c_int,
        out: *mut c_char,
        out_capacity: c_int,
    ) -> c_int;
    pub fn needle_stream_transcribe_process(
        pcm: *const c_float,
        samples: c_int,
        language: *const c_char,
        keywords: *const c_char,
        out: *mut c_char,
        out_capacity: c_int,
    ) -> c_int;
    pub fn needle_stream_transcribe_stop(out: *mut c_char, out_capacity: c_int) -> c_int;
    pub fn needle_embed(
        input: *const c_char,
        pcm: *const c_float,
        samples: c_int,
        out: *mut c_float,
        out_capacity: c_int,
    ) -> c_int;
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    const SRC: &str = include_str!("lib.rs");
    /// SHA-256 of the pinned `needle.h`.
    const HEADER_SHA256: &str = "a27dac4b3239eead91d124b7f7dbce5495c13019903fd817b68c16d39b055a99";

    fn header() -> Option<String> {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../vendor/needle/linux-x86_64/needle.h");
        std::fs::read_to_string(p).ok()
    }

    /// Every `NEEDLE_API` function of the header must be bound (skipped without the vendored header).
    #[test]
    fn bindings_cover_the_header() {
        let Some(h) = header() else {
            eprintln!("needle.h not vendored; skipping (run scripts/fetch-needle.sh)");
            return;
        };
        let mut n = 0;
        for line in h.lines().filter(|l| l.starts_with("NEEDLE_API")) {
            let name = line
                .split('(')
                .next()
                .and_then(|s| s.split_whitespace().last())
                .unwrap_or("")
                .trim_start_matches('*');
            assert!(
                SRC.contains(&format!("pub fn {name}(")),
                "header function {name} is not bound"
            );
            n += 1;
        }
        assert_eq!(
            n, 11,
            "header function count changed; re-audit the bindings"
        );
    }

    #[test]
    fn header_is_the_pinned_one() {
        let Some(h) = header() else { return };
        // Cheap structural pin (no sha crate): length + hash recorded in HEADER_SHA256.
        assert_eq!(
            h.len(),
            4528,
            "needle.h changed (expected sha256 {HEADER_SHA256})"
        );
    }
}
