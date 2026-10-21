//! Reserve a portable CLI stack for instrumented Windows builds.

/// Keeps the native CLI's bounded debug dispatcher within its executable stack reserve.
fn main() {
    // MSVC defaults executable stacks to 1 MiB, while Rust test threads use 2 MiB.
    // LLVM branch instrumentation can overflow the smaller reserve during create.
    // This is virtual address space; committed stack pages still grow on demand.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bin=pm-rust=/STACK:8388608");
    }
}
