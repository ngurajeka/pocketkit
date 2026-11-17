{
  description = "MinUI paks for the Miyoo Mini / Mini+";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { nixpkgs, rust-overlay, ... }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forAll = f: nixpkgs.lib.genAttrs systems (system:
        f (import nixpkgs { inherit system; overlays = [ rust-overlay.overlays.default ]; }));
    in
    {
      devShells = forAll (pkgs:
        let
          rust = pkgs.rust-bin.stable.latest.default.override {
            extensions = [ "rust-src" "rust-analyzer" "clippy" "rustfmt" ];
            # Miyoo Mini / Mini+ (Cortex-A7, hard-float), statically linked against musl
            targets = [ "armv7-unknown-linux-musleabihf" ];
          };
          # minifb (desktop preview) dlopens these at runtime on Linux
          desktopLibs = with pkgs; pkgs.lib.optionals stdenv.hostPlatform.isLinux [
            libX11 libXcursor libXrandr libXi libxkbcommon wayland
          ];
        in
        {
          default = pkgs.mkShell {
            packages = [ rust pkgs.pkg-config ] ++ desktopLibs;
            LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath desktopLibs;
            # package.sh: plain cargo + rust-lld against Rust's bundled (self-contained)
            # musl — no Docker/cross, no zig, no full C cross toolchain.
            BUILD_CMD = "cargo build";
            CARGO_TARGET_ARMV7_UNKNOWN_LINUX_MUSLEABIHF_LINKER = "rust-lld";
            # Bare clang for crates with small, libc-free C parts (e.g. ring, used by
            # rustls for HTTPS). There are no musl headers here, so C code that
            # #includes libc headers still won't build.
            CC_armv7_unknown_linux_musleabihf = "${pkgs.llvmPackages.clang-unwrapped}/bin/clang";
            CFLAGS_armv7_unknown_linux_musleabihf = "--target=armv7-unknown-linux-musleabihf";
            AR_armv7_unknown_linux_musleabihf = "${pkgs.llvmPackages.llvm}/bin/llvm-ar";
          };
        });
    };
}
