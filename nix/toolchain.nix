# Official Rust 1.99.0 components, pinned independently of nixpkgs' compiler age.
{ pkgs }:
let
  arch = if pkgs.stdenv.hostPlatform.system == "aarch64-linux" then "aarch64" else "x86_64";
  hashes = {
    x86_64 = { rustc = "77171ba2a0345fdf2abc4fedda55d6de078dae7a68527c28be8c77dcc9604bd5"; cargo = "d7674918d28093097614cd9728b6ca60db9ea3038f640f0bd1e9a4188c7568ce"; rust-std = "3e58dff2d0b72196b5ea4e90536e174d400de88564a52694686b81e091169933"; };
    aarch64 = { rustc = "89c0f1a3a44df63c95e5d5f2ba093d0a12c5f5f57f8635f8a7a0ce50cf86600e"; cargo = "113229fcc16cc1ba004923a4cc275fbf3281dafaaf8b97269f9c00189d497fd4"; rust-std = "1cf7e2ef58ed1cfa6f1adea18e155cbf254c1951f39e5f5e3376879f55fd64a4"; };
  };
  sources = map (component: pkgs.fetchurl {
    url = "https://static.rust-lang.org/dist/${component}-1.99.0-${arch}-unknown-linux-gnu.tar.xz";
    sha256 = hashes.${arch}.${component};
  }) [ "rustc" "cargo" "rust-std" ];
in pkgs.stdenv.mkDerivation {
  pname = "apron-rust-toolchain"; version = "1.99.0";
  passthru.targetPlatforms = [ pkgs.stdenv.hostPlatform.system ];
  dontUnpack = true; dontStrip = true;
  nativeBuildInputs = [ pkgs.autoPatchelfHook ];
  buildInputs = [ pkgs.stdenv.cc.cc.lib pkgs.zlib ];
  installPhase = ''
    mkdir -p "$out"
    for component in ${pkgs.lib.concatStringsSep " " sources}; do
      tar xf "$component"
    done
    for component in rustc-* cargo-* rust-std-*; do
      bash "$component/install.sh" --prefix="$out" --disable-ldconfig
    done
  '';
}
