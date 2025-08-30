{
    inputs = {
        nixpkgs-unstable.url = "github:NixOS/nixpkgs/nixos-unstable"; # requires Rust 1.87 or higher (https://blog.rust-lang.org/2025/05/15/Rust-1.87.0/#precise-capturing-use-in-impl-trait-in-trait-definitions)
    };
    outputs = { self, nixpkgs-unstable }: let
        supportedSystems = [
            "aarch64-darwin"
            "aarch64-linux"
            "x86_64-darwin"
            "x86_64-linux"
        ];
        forEachSupportedSystem = f: nixpkgs-unstable.lib.genAttrs supportedSystems (system: f {
            pkgs = import nixpkgs-unstable { inherit system; };
        });
    in {
        packages = forEachSupportedSystem ({ pkgs, ... }: let
            manifest = (pkgs.lib.importTOML ./Cargo.toml).package;
        in {
            default = pkgs.rustPlatform.buildRustPackage {
                buildFeatures = [
                    "nixos"
                ];
                cargoLock = {
                    allowBuiltinFetchGit = true; # allows omitting cargoLock.outputHashes
                    lockFile = ./Cargo.lock;
                };
                pname = "sil";
                src = ./.;
                version = manifest.version;
            };
        });
    };
}
