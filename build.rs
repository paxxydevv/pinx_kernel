// Build-time configuration for the Pinx kernel.
//
// This is the Rust replacement for the old ./configure script: it validates
// the requested runtime log level and hands it to the compiler through an
// environment variable, so the kernel can apply it in klog_init() without a
// second configuration file. Everything else is expressed as Cargo features
// declared in Cargo.toml.

use std::env;

const LEVELS: [&str; 6] = ["trace", "debug", "info", "warn", "error", "none"];

fn main() {
    println!("cargo:rerun-if-env-changed=PINX_LOG_LEVEL");

    let level = env::var("PINX_LOG_LEVEL").unwrap_or_else(|_| String::from("info"));

    if !LEVELS.contains(&level.as_str()) {
        panic!(
            "PINX_LOG_LEVEL: '{level}' is not one of: {}",
            LEVELS.join(" ")
        );
    }

    println!("cargo:rustc-env=PINX_LOG_LEVEL={level}");
    println!("cargo:rerun-if-changed=linker.ld");
    println!("cargo:rerun-if-changed=build.rs");
}
