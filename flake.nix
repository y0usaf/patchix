{
  description = "Declarative config file patcher for Nix";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs = {
    self,
    nixpkgs,
  }: let
    systems = ["x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin"];
    forEachSystem = nixpkgs.lib.genAttrs systems;
    pkgsForEach = nixpkgs.legacyPackages;
  in {
    packages = forEachSystem (system: {
      patchix = pkgsForEach.${system}.callPackage ./nix/package.nix {};
      default = self.packages.${system}.patchix;
    });

    nixosModules.default = import ./nix/module.nix;

    devShells = forEachSystem (system: {
      default = pkgsForEach.${system}.callPackage ./nix/shell.nix {};
    });
  };
}
