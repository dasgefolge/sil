{
    inputs.flake.url = "github:fenhl/flake";
    outputs = attrs: attrs.flake.lib {
        devShells.default = { pkgs, ... }: {
            packages = with pkgs; [
                cargo
                python3 # required for pre-commit script
            ];
        };
        packages.default = { pkgs, ... }: let
            manifest = (pkgs.lib.importTOML ./Cargo.toml).package;
        in pkgs.rustPlatform.buildRustPackage {
            inherit (manifest) version;
            pname = "sil";
            buildFeatures = [
                "nixos"
            ];
            cargoLock = {
                allowBuiltinFetchGit = true; # allows omitting cargoLock.outputHashes
                lockFile = ./Cargo.lock;
            };
            src = ./.;
        };
    };
}
