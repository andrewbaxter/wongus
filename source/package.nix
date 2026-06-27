{ pkgs, lib }:
let
  fenix =
    import
      (fetchTarball "https://github.com/nix-community/fenix/archive/1a79901b0e37ca189944e24d9601c8426675de50.zip")
      { };
  naersk =
    pkgs.callPackage
      (fetchTarball "https://github.com/nix-community/naersk/archive/378614f37a6bee5a3f2ef4f825a73d948d3ae921.zip")
      (
        let
          toolchain = fenix.combine [
            fenix.stable.rustc
            fenix.stable.cargo
          ];
        in
        {
          rustc = toolchain;
          cargo = toolchain;
        }
      );
  layershell = (
    pkgs.gtk-layer-shell.overrideAttrs (old: {
      name = "gtk-layer-shell-override";
      src = pkgs.fetchFromGitHub {
        owner = "wmww";
        repo = "gtk-layer-shell";
        # issue 211
        rev = "0bc31ca504377fa716d6e32cb04afc5350851414";
        sha256 = "sha256-bA0Pqp6+msvCz/VKzHe9dq6jvtvlh1giMgtf4byQP6A=";
      };
    })
  );
in
naersk.buildPackage (
  { }
  // {
    root = ./.;
    nativeBuildInputs = [
      pkgs.pkg-config
      pkgs.cargo
      pkgs.rustc
      pkgs.rustPlatform.bindgenHook
      pkgs.makeWrapper
      # This magically wraps the program with env vars from (somewhere?)
      pkgs.wrapGAppsHook3
    ];
    buildInputs = [
      pkgs.at-spi2-atk
      pkgs.atkmm
      pkgs.cairo
      pkgs.gdk-pixbuf
      pkgs.glib
      pkgs.gtk3
      pkgs.harfbuzz
      pkgs.librsvg
      pkgs.libsoup_3
      pkgs.pango
      pkgs.webkitgtk_4_1
      layershell
      pkgs.openssl
    ];
  }
)
