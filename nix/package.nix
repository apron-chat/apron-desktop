{ pkgs }:
let
  toolchain = import ./toolchain.nix { inherit pkgs; };
  platform = pkgs.makeRustPlatform { cargo = toolchain; rustc = toolchain; };
  libraries = with pkgs; [ fontconfig freetype wayland libxkbcommon vulkan-loader openssl
    libx11 libxcursor libxrandr libxi libxcb ];
in platform.buildRustPackage {
  pname = "apron-desktop"; version = "0.1.0";
  src = pkgs.lib.cleanSourceWith {
    src = ../.;
    filter = path: type: pkgs.lib.cleanSourceFilter path type &&
      !(builtins.elem (baseNameOf path) [ "target" "artifacts" ".reference" ]);
  };
  cargoLock.lockFile = ../Cargo.lock;
  nativeBuildInputs = with pkgs; [ pkg-config cmake clang makeWrapper ];
  buildInputs = libraries;
  LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
  # Network fixtures bind ephemeral loopback sockets; no external services.
  cargoTestFlags = [ "--no-default-features" ];
  postInstall = ''
    install -Dm644 data/chat.apron.GpuiExperiment.desktop "$out/share/applications/chat.apron.GpuiExperiment.desktop"
    install -Dm644 data/chat.apron.GpuiExperiment.svg "$out/share/icons/hicolor/scalable/apps/chat.apron.GpuiExperiment.svg"
    wrapProgram "$out/bin/apron-gpui" \
      --prefix LD_LIBRARY_PATH : "/run/opengl-driver/lib:${pkgs.lib.makeLibraryPath libraries}" \
      --prefix PATH : "${pkgs.lib.makeBinPath [ pkgs.libsecret pkgs.xdg-utils pkgs.wl-clipboard ]}"
  '';
  meta = { description = "Native Wayland client for Apron"; license = pkgs.lib.licenses.mit;
    platforms = [ "x86_64-linux" "aarch64-linux" ]; mainProgram = "apron-gpui"; };
}
