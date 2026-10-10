{
  description = "Apron GPUI comparison development shell";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/7c8764b7c7b09b34f632464276218ef9090eaa11";
  outputs = { nixpkgs, ... }: let
    systems = [ "x86_64-linux" "aarch64-linux" ];
  in {
    devShells = nixpkgs.lib.genAttrs systems (system: let
      pkgs = nixpkgs.legacyPackages.${system};
      libraries = with pkgs; [ fontconfig freetype wayland libxkbcommon vulkan-loader mesa openssl
        libx11 libx11.dev libxcursor libxrandr libxi libxcb ];
    in { default = pkgs.mkShell {
      packages = with pkgs; [ rustup pkg-config clang cmake gnumake (python3.withPackages (ps: [ ps.websockets ])) weston vulkan-tools wl-clipboard ];
      buildInputs = libraries;
      LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
      LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath libraries;
      shellHook = ''
        echo 'Use the pinned rust-toolchain.toml: cargo build --release --locked'
        echo 'Run inside your existing Wayland session; hardware drivers come from NixOS.'
      '';
    }; });
  };
}
