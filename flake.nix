{
  description = "Rowing lineup generator — constraint-solver-backed boat assignment for GGRC";

  inputs = {
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    nixpkgs.url = "nixpkgs/nixos-unstable";
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ rust-overlay.overlays.default ];
        pkgs = import nixpkgs { inherit system overlays; };
        rust = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        inputs = [
          rust
          pkgs.rust-analyzer
          pkgs.diesel-cli
          pkgs.sqlite
          pkgs.openssl
          pkgs.pkg-config
          pkgs.webkitgtk_4_1
          pkgs.tailwindcss
        ];
        server = pkgs.rustPlatform.buildRustPackage {
          pname = "lineup_server";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          cargoBuildFlags = [ "-p" "lineup_server" ];
          doCheck = false; # tests need fixtures + DB
        };
      in
      {
        packages = {
          default = server;
          docker = pkgs.dockerTools.buildLayeredImage {
            name = "lineup-generator";
            tag = "latest";
            contents = [ server ];
            extraCommands = ''
              mkdir -p data data/demos app/public
              cp -r ${./crates/server/public}/* app/public/
            '';
            config = {
              Cmd = [ "${server}/bin/lineup_server" ];
              Env = [
                "HOST=0.0.0.0"
                "PORT=8080"
                "MASTER_DB=/data/master.db"
                "DATA_DIR=/data"
                "PUBLIC_DIR=/app/public"
              ];
              ExposedPorts = { "8080/tcp" = {}; };
            };
          };
        };

        devShell = pkgs.mkShell {
          packages = inputs;
          nativeBuildInputs = with pkgs; [
            flyctl
            skopeo
            cargo-nextest
            cargo-watch
            (writeShellScriptBin "dump-snapshot" ''
              cargo run -p lineup_cli -- "$@"
            '')
            (writeShellScriptBin "db-reset" ''
              rm -f lineup.sql
              cargo run -p lineup_cli
            '')
          ];
          DATABASE_URL = "lineup.sql";
          shellHook = ''
            git config core.hooksPath .githooks
          '';
        };
      }
    );
}
