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
            # Miyoo Mini / Mini+ (Cortex-A7, hard-float): static musl for most paks,
            # glibc (via the Miyoo toolchain below) for the music pak's vendor audio library.
            targets = [ "armv7-unknown-linux-musleabihf" "armv7-unknown-linux-gnueabihf" ];
          };
          # The Miyoo Mini toolchain MinUI itself builds with: Linaro GCC 8.3 and a
          # glibc 2.28 sysroot holding the SigmaStar vendor libraries (libmi_ao for
          # audio, ...). Prebuilt for x86_64 Linux only.
          miyooToolchain = pkgs.stdenv.mkDerivation {
            pname = "miyoomini-toolchain";
            version = "0.0.3";
            src = pkgs.fetchurl {
              url = "https://github.com/shauninman/miyoomini-toolchain-buildroot/releases/download/v0.0.3/miyoomini-toolchain.tar.xz";
              hash = "sha256-4Vi2694Mkz7NV7NnYPuGAvrUF2oJ+2FVnce8/+ZiTy4=";
            };
            nativeBuildInputs = [ pkgs.autoPatchelfHook ];
            buildInputs = [ pkgs.stdenv.cc.cc.lib pkgs.zlib ];
            # Only the host (x86_64) tools get patched; the ARM sysroot must stay as is.
            dontAutoPatchelf = true;
            dontStrip = true;
            dontPatchShebangs = true;
            # The sysroot mirrors a device filesystem: links like /dev/stdin or
            # /etc/mtab point into /proc and only resolve on the device.
            dontCheckForBrokenSymlinks = true;
            installPhase = ''
              mkdir -p $out
              cp -a . $out/
            '';
            # The bundled gdb wants old ncurses/python2 libraries; it isn't needed.
            autoPatchelfIgnoreMissingDeps = [ "libtinfo.so.5" "libncursesw.so.5" "libpython2.7.so.1.0" "libexpat.so.1" ];
            postFixup = ''
              autoPatchelf $out/bin $out/libexec $out/arm-linux-gnueabihf/bin
            '';
          };
          hasMiyooToolchain = pkgs.stdenv.hostPlatform.system == "x86_64-linux";
          # minifb (desktop preview) dlopens these at runtime on Linux
          desktopLibs = with pkgs; pkgs.lib.optionals stdenv.hostPlatform.isLinux [
            libX11 libXcursor libXrandr libXi libxkbcommon wayland
          ];
        in
        {
          default = pkgs.mkShell ({
            # android-tools: adb, used by deploy.sh to copy paks to the Mini over Wi-Fi.
            packages = [ rust pkgs.pkg-config pkgs.android-tools ] ++ desktopLibs;
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
          } // pkgs.lib.optionalAttrs hasMiyooToolchain {
            # glibc target (music pak): link and compile C with the Miyoo toolchain.
            MIYOO_TOOLCHAIN = "${miyooToolchain}";
            CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_LINKER = "${miyooToolchain}/bin/arm-linux-gnueabihf-gcc";
            CC_armv7_unknown_linux_gnueabihf = "${miyooToolchain}/bin/arm-linux-gnueabihf-gcc";
            AR_armv7_unknown_linux_gnueabihf = "${miyooToolchain}/bin/arm-linux-gnueabihf-ar";
          });
        });
    };
}
