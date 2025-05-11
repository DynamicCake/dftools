{
  description = "Rust dev environment";

  inputs = {nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";};

  outputs = {
    self,
    nixpkgs,
  }: let
    allSystems = ["x86_64-linux" "aarch64-darwin"];
    # Found from
    # https://git.dyncake.dev/cake/devshell-basic
    forAllSystems = fn:
      nixpkgs.lib.genAttrs allSystems
      (system: fn {pkgs = import nixpkgs {inherit system;};});
  in {
    devShells = forAllSystems ({pkgs}: {
      default = pkgs.mkShell {
        # name = "nix";
        nativeBuildInputs = with pkgs; [
          cargo
          rustc
          pkg-config
          openssl
          sqlx-cli
        ];
      };
    });
  };
}
