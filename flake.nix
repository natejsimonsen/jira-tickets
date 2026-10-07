{
  description = "A fast desktop app for Jira tickets, grooming, and roadmaps";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  outputs = { self, nixpkgs }:
    let
      systems = [ "aarch64-darwin" "x86_64-darwin" "x86_64-linux" "aarch64-linux" ];
      forAll = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in {
      packages = forAll (pkgs: rec {
        jira-tickets = pkgs.callPackage ./nix/package.nix { };
        default = jira-tickets;
      });
      apps = forAll (pkgs: {
        default = { type = "app"; program = "${self.packages.${pkgs.stdenv.hostPlatform.system}.default}/bin/jira-tickets"; };
      });
      devShells = forAll (pkgs: { default = pkgs.callPackage ./nix/shell.nix { }; });
      formatter = forAll (pkgs: pkgs.nixfmt);
    };
}
