//! Bakes the build time into the binary (see `net::effective_now`).
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    println!("cargo:rustc-env=BUILD_UNIX_TIME={now}");
    println!("cargo:rerun-if-changed=build.rs");
}
