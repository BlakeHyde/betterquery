{
  description = "BetterQuery, an s-expression query and transformation language for ordered JSON";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forEachSystem =
        f: nixpkgs.lib.genAttrs systems (system: f system nixpkgs.legacyPackages.${system});
    in
    {
      packages = forEachSystem (
        _system: pkgs:
        let
          cargoRoot = "bq";
        in
        {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "betterquery";
            version = "0.1.0";

            src = ./.;

            # The Cargo workspace lives in bq/, not the repository root.
            inherit cargoRoot;
            buildAndTestSubdir = cargoRoot;
            cargoLock.lockFile = ./bq/Cargo.lock;

            meta = {
              description = "S-expression query and transformation language for ordered JSON";
              homepage = "https://github.com/BlakeHyde/betterquery";
              license = pkgs.lib.licenses.gpl3Only;
              mainProgram = "bq-web";
            };
          };
        }
      );

      devShells = forEachSystem (
        _system: pkgs: {
          default = pkgs.mkShell {
            packages = with pkgs; [
              rustc
              cargo
              clippy
              rustfmt
              rust-analyzer
            ];

            shellHook = ''
              export RUST_SRC_PATH=${pkgs.rustPlatform.rustLibSrc}
            '';
          };
        }
      );

      checks = forEachSystem (system: _pkgs: {
        betterquery = self.packages.${system}.default;
      });
    };
}
