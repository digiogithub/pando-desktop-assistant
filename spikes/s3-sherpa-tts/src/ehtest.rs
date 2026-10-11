//! Isolates C++ exception handling when libstdc++-built code (g++ shim, see
//! build.rs) and libc++-built libneedle share one Rust binary.
//! Build with S3_ONLY_SHIM=1 to get the shim alone (baseline).
use std::ffi::{c_int, c_uchar, c_ulonglong};
unsafe extern "C" {
    fn eh_shim_libstdcxx(n: c_int) -> c_int;
}
#[cfg(not(s3_only_shim))]
unsafe extern "C" {
    fn needle_load(cact: *const c_uchar, n: c_ulonglong) -> c_int;
}
fn main() {
    println!("start");
    #[cfg(not(s3_only_shim))]
    if std::env::args().nth(1).as_deref() != Some("shim-first") {
        let junk = [0u8; 64];
        println!("needle_load(junk) = {}", unsafe {
            needle_load(junk.as_ptr(), 64)
        });
    }
    println!(
        "eh_shim_libstdcxx(1000) caught = {} (expect 2000)",
        unsafe { eh_shim_libstdcxx(1000) }
    );
    let _ = (c_uchar::MIN, c_ulonglong::MIN);
}
