{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane.url = "github:ipetkov/crane";
    devshell = {
      url = "github:numtide/devshell";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { nixpkgs, rust-overlay, devshell, flake-utils, crane, ... }: let
    # define dendrite in an overlay
    overlay = prev: final: let
      prev' = prev.extend (import rust-overlay);
    in {
      dendrite = prev'.callPackage ./package.nix { inherit crane; };
    };
    
    per-system = flake-utils.lib.eachDefaultSystem (system: let
      # dendrite from flake nixpkgs
      dendrite = (import nixpkgs { inherit system; overlays = [ overlay ]; }).dendrite;
      
      # pkgs for devshell
      pkgs = import nixpkgs {
        inherit system;
        overlays = [ (import rust-overlay) devshell.overlays.default ];
      };

      dev-toolchain = p: p.rust-bin.selectLatestNightlyWith (toolchain: toolchain.default.override {
        extensions = [ "rust-src" "rust-analyzer" ];
      });

      # avoids the pkg-config hook that devshell doesn't have
      make-pkg-config-path = packages:
        pkgs.lib.concatStringsSep ":" (
          pkgs.lib.concatMap
            (pkg: map (sub: "${pkgs.lib.getDev pkg}/${sub}") [ "lib/pkgconfig" "share/pkgconfig" ])
            packages
        );

      common-devshell-packages = with pkgs; [
        pkg-config clang mold
        fontconfig
        ( dev-toolchain pkgs )
      ];
      linux-packages = with pkgs; [
        pkg-config

        vulkan-headers vulkan-loader
        vulkan-tools vulkan-tools-lunarg
        vulkan-extension-layer
      ];
      linux-runtime-libs = with pkgs; [
        wayland libxkbcommon
        libx11 libxcb libxcursor libxi
        vulkan-loader libglvnd
        fontconfig
      ];

      linux-devshell = pkgs.devshell.mkShell (let
        packages = common-devshell-packages ++ linux-packages ++ linux-runtime-libs;
      in {
        inherit packages;
        motd = "\n  Welcome to the {2}$(basename $PRJ_ROOT){reset} shell.\n";
        env = [
          { name = "LD_LIBRARY_PATH"; value = pkgs.lib.makeLibraryPath packages; }
          { name = "PKG_CONFIG_PATH"; value = make-pkg-config-path packages; }
        ];
      });

      darwin-devshell = pkgs.mkShell (let
        packages = common-devshell-packages ++ [ pkgs.apple-sdk ];
      in {
        nativeBuildInputs = packages;
        LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath packages;
      });
    in {
      packages = {
        inherit dendrite;
        default = dendrite;
      };
      checks = { inherit dendrite; };
      devShell = if pkgs.stdenv.hostPlatform.isLinux then linux-devshell else darwin-devshell;
    });
  in {
    overlays.default = overlay;
  } // per-system;
}

