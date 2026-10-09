{
  description = "Apron Native — GTK4 / libadwaita Wayland client";
  # Exact source revision: no floating channel dependency.
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/7c8764b7c7b09b34f632464276218ef9090eaa11";
  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      each = nixpkgs.lib.genAttrs systems;
      package = pkgs: pkgs.python3Packages.buildPythonApplication {
        pname = "apron-native";
        version = "0.1.0";
        src = self;
        pyproject = true;
        build-system = [ pkgs.python3Packages.setuptools ];
        dependencies = with pkgs.python3Packages; [ pygobject3 markdown-it-py ];
        nativeBuildInputs = [ pkgs.wrapGAppsHook4 pkgs.gobject-introspection ];
        buildInputs = [ pkgs.gtk4 pkgs.libadwaita pkgs.libsoup_3 pkgs.libsecret ];
        dontWrapGApps = true;
        preFixup = ''makeWrapperArgs+=( "''${gappsWrapperArgs[@]}" )'';
        postInstall = ''
          install -Dm644 data/chat.apron.Native.desktop $out/share/applications/chat.apron.Native.desktop
          install -Dm644 data/chat.apron.Native.svg $out/share/icons/hicolor/scalable/apps/chat.apron.Native.svg
        '';
        nativeCheckInputs = [ pkgs.python3Packages.markdown-it-py ];
        checkPhase = ''python -m unittest discover -s tests -p test_model.py -v'';
        meta = {
          description = "Native GTK4 Apron chat client";
          mainProgram = "apron-native";
          platforms = systems;
          license = pkgs.lib.licenses.mit;
        };
      };
    in {
      packages = each (system: { default = package nixpkgs.legacyPackages.${system}; });
      apps = each (system: { default = { type = "app"; program = "${self.packages.${system}.default}/bin/apron-native"; }; });
      devShells = each (system:
        let pkgs = nixpkgs.legacyPackages.${system}; in {
          default = pkgs.mkShell {
            inputsFrom = [ self.packages.${system}.default ];
            packages = [ (pkgs.python3.withPackages (p: [ p.pygobject3 p.markdown-it-py p.websockets p.setuptools ])) pkgs.weston pkgs.grim ];
            shellHook = ''
              export GI_TYPELIB_PATH="${pkgs.gtk4}/lib/girepository-1.0:${pkgs.libadwaita}/lib/girepository-1.0:${pkgs.libsoup_3}/lib/girepository-1.0:${pkgs.libsecret}/lib/girepository-1.0:$GI_TYPELIB_PATH"
            '';
          };
        });
    };
}
