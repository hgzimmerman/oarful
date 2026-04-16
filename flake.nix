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
      in
      {
        devShell = pkgs.mkShell {
          packages = inputs;
          nativeBuildInputs = with pkgs; [
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
        };
      }
    );
}
