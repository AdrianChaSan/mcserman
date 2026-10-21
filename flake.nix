{
  description = "A powerful, host-independent Minecraft Dedicated Server Manager in Rust";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
  };

  outputs = { self, nixpkgs }:
    let
      supportedSystems = [ "x86_64-linux" "aarch64-linux" ];
      forAllSystems = nixpkgs.lib.genAttrs supportedSystems;
    in
    {
      packages = forAllSystems (system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.callPackage ./nix/package.nix { };
          mcserman = self.packages.${system}.default;
        });

      nixosModules = {
        default = import ./nix/module.nix { inherit self; };
        mcserman = self.nixosModules.default;
      };

      devShells = forAllSystems (system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.mkShell {
            nativeBuildInputs = with pkgs; [
              rustc
              cargo
              clippy
              rustfmt
              pkg-config
            ];
            buildInputs = with pkgs; [
              temurin-bin-25
              temurin-bin-21
              temurin-bin-17
              temurin-bin-8
            ];
            shellHook = ''
              export MCSERVER_JAVA_25_PATH="${pkgs.temurin-bin-25}/bin/java"
              export MCSERVER_JAVA_21_PATH="${pkgs.temurin-bin-21}/bin/java"
              export MCSERVER_JAVA_17_PATH="${pkgs.temurin-bin-17}/bin/java"
              export MCSERVER_JAVA_8_PATH="${pkgs.temurin-bin-8}/bin/java"
              export JAVA_HOME="${pkgs.temurin-bin-21}"
            '';
          };
        });

      apps = forAllSystems (system: {
        default = {
          type = "app";
          program = "${self.packages.${system}.default}/bin/mcserman";
          meta.description = "A powerful, host-independent Minecraft Dedicated Server Manager in Rust";
        };
      });
    };
}
