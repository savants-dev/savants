{
  description = "Savants - the context engine for AI coding agents";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };
        rustToolchain = pkgs.rust-bin.stable.latest.default;
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "savants";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;

          nativeBuildInputs = with pkgs; [
            pkg-config
            cmake
          ];

          buildInputs = with pkgs; [
            openssl
          ] ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
            pkgs.darwin.apple_sdk.frameworks.Security
            pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
          ];

          # ONNX runtime for embeddings
          ONNXRUNTIME_LIB_PATH = "${pkgs.onnxruntime}/lib";

          # Skip tests that need network
          checkFlags = [ "--skip=integration" ];

          meta = with pkgs.lib; {
            description = "The context engine for AI coding agents. Semantic search, blast radius, call graph analysis.";
            homepage = "https://savants.dev";
            license = licenses.mit;
            mainProgram = "savants";
          };
        };

        # nix develop - gives you a shell with all deps
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustToolchain
            pkg-config
            openssl
            cmake
          ];
        };
      }
    );
}
