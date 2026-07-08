# Nix flake for hermetic development environment
{
  description = "MegaGate development environment";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };

        rust-toolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rustfmt" "clippy" ];
        };
      in {
        devShells.default = pkgs.mkShell {
          name = "megagate-dev";
          buildInputs = with pkgs; [
            rust-toolchain
            cargo
            bazel_7
            protobuf
            nodejs_20
            pkg-config
            openssl
          ];

          SHELL_HOOK = ''
            export RUSTUP_TOOLCHAIN=stable
            export BAZEL_VERSION=7.4.0
            echo "MegaGate development environment ready!"
            echo "Available commands:"
            echo "  cargo check --workspace"
            echo "  cargo test --workspace"
            echo "  bazel build //..."
            echo "  bazel test //..."
          '';
        };

        packages.default = pkgs.stdenv.mkDerivation {
          name = "megagate";
          src = self;
          nativeBuildInputs = with pkgs; [
            cargo
            bazel_7
            rust-toolchain
          ];
          buildPhase = ''
            bazel build //crates/megagate-cli:megagate
          '';
          installPhase = ''
            mkdir -p $out/bin
            cp bazel-bin/crates/megagate-cli/megagate $out/bin/
          '';
        };
      });
}
