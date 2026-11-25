// The official macOS SDK publishes @rpath-based dylib install names.
// Embed the standard installed framework path into all native media binaries;
// no DYLD environment override, absolute user directory or untrusted search path.
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-arg-bins=-Wl,-rpath,/Library/Frameworks/GStreamer.framework/Versions/1.0/lib");
    }
}
