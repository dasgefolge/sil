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
            nativeBuildInputs = with pkgs; [
                makeWrapper # required for wrapProgram in postFixup hook
            ];
            postFixup = ''
                wrapProgram $out/bin/sil \
                    --prefix LD_LIBRARY_PATH : ${pkgs.lib.makeLibraryPath (with pkgs; [
                        libxkbcommon # required to fix runtime error “XKBNotFound”
                        wayland # required to fix runtime error “The wayland library could not be loaded”
                    ])}
            '';
            src = ./.;
        };
    };
}
