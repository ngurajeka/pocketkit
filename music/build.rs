//! For the device build, compiles the tiny C shim around the vendor MI_AO audio
//! API and links the SigmaStar libraries statically (only glibc stays dynamic).
//! Needs the Miyoo toolchain (MIYOO_TOOLCHAIN, CC_armv7_unknown_linux_gnueabihf).

fn main() {
    println!("cargo:rerun-if-changed=src/ao_shim.c");
    if std::env::var_os("CARGO_FEATURE_DEVICE").is_none() {
        return;
    }
    println!("cargo:rerun-if-env-changed=MIYOO_TOOLCHAIN");
    let toolchain = std::env::var("MIYOO_TOOLCHAIN").expect("MIYOO_TOOLCHAIN must point at the Miyoo Mini toolchain");
    println!("cargo:rustc-link-search=native={toolchain}/arm-linux-gnueabihf/libc/usr/lib");
    cc::Build::new().file("src/ao_shim.c").warnings(true).compile("ao_shim");
    for lib in ["mi_ao", "mi_sys", "mi_common"] {
        println!("cargo:rustc-link-lib=static={lib}");
    }
    for lib in ["pthread", "dl", "rt", "m"] {
        println!("cargo:rustc-link-lib=dylib={lib}");
    }
}
