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
        # https://github.com/wmww/gtk-layer-shell/pull/198/commits
        rev = "56aae1e4c41d78cd535c7c8f75883fbce95d7de3";
        hash = "sha256-9/pd4odCtFoIlJECHAHcpzdth1/ustaYHh7Nu4YJymo=";
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
    postInstall = ''
      ${pkgs.coreutils}/bin/ln -s ${layershell} $out/used_layershell
    '';
  }
)
