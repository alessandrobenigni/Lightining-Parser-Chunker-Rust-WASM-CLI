# Nix flake for reproducible builds of parser-chunker.
#
# This is a placeholder. A full Nix setup requires:
# 1. Use crane or naersk for Rust builds
# 2. Pin nixpkgs to a specific commit for reproducibility
# 3. Add system dependencies (openssl, pkg-config)
# 4. Configure cross-compilation targets
#
# To use once fully configured:
#   nix build .#parser-chunker
#   nix develop  # enter dev shell
#
# References:
# - https://crane.dev/
# - https://github.com/nix-community/naersk
{
  description = "Parser Chunker - High-performance document parser and chunker";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
    # crane.url = "github:ipetkov/crane";  # Uncomment for Rust builds
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };
        rustToolchain = pkgs.rust-bin.stable."1.85.0".default;
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = [
            rustToolchain
            pkgs.pkg-config
            pkgs.openssl
          ];
        };

        # TODO: Add package build using crane or naersk
        # packages.default = crane.lib.${system}.buildPackage {
        #   src = ./.;
        #   buildInputs = [ pkgs.openssl pkgs.pkg-config ];
        # };
      }
    );
}
