{
  description = "Rust CLAP workspace (crane)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    crane.url = "github:ipetkov/crane";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, crane, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        lib = pkgs.lib;

        craneLib = crane.mkLib pkgs;

        # Workspace sources (filters target/, .git/, etc.)
        src = craneLib.cleanCargoSource ./.;

        # System deps typically needed for X11 + OpenGL UI stacks on Linux
        guiDeps = with pkgs; [
          pkg-config
          libxkbcommon
          wayland
          xorg.libX11
          xorg.libxcb
          xorg.libXext
          xorg.libXcursor
          xorg.libXrandr
          xorg.libXi
          xorg.libXinerama
          libglvnd
          mesa
        ];

        commonArgs = {
          inherit src;
          strictDeps = true;
          nativeBuildInputs = [ pkgs.pkg-config ];
          buildInputs = guiDeps;
          cargoLock = ./Cargo.lock; # in newer crane, this is a path (not an attrset)
        };

        # Build all workspace dependencies once, then reuse.
        cargoArtifacts = craneLib.buildDepsOnly commonArgs; # :contentReference[oaicite:1]{index=1}

        # CHANGE THIS to your actual plugin crate package name from Cargo.toml
        pluginCrate = "cave";

        pluginDrv = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;

          pname = pluginCrate;
          # Build only that crate in the workspace:
          cargoExtraArgs = "-p ${pluginCrate}";
          doCheck = false;

          installPhase = ''
            runHook preInstall

            # Find the produced shared library. (Your crate must be crate-type = ["cdylib"])
            so=$(ls target/release/lib${pluginCrate}.so 2>/dev/null || true)
            if [ -z "$so" ]; then
              echo "Could not find target/release/lib${pluginCrate}.so"
              echo "Check that ${pluginCrate} builds a cdylib and the crate name matches."
              exit 1
            fi

            mkdir -p $out/lib/clap
            # CLAP plugins on Linux are typically a single shared object with .clap extension.
            cp -v "$so" "$out/lib/clap/${pluginCrate}.clap"

            runHook postInstall
          '';
        });

      in
      {
        packages.default = pluginDrv;

        # For `nix run` you can add an app (optional)
        # apps.default = flake-utils.lib.mkApp { drv = pluginDrv; };

        devShells.default = craneLib.devShell {
          # craneLib.devShell provides cargo/rustc; add extra tools here.
          packages = guiDeps ++ [ pkgs.gdb pkgs.clang ];
        };
      });
}
