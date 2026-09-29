{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    crane.url = "github:ipetkov/crane";
    devshell.url = "github:numtide/devshell";
  };

  outputs = { nixpkgs, rust-overlay, devshell, flake-utils, crane, ... }: 
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = import nixpkgs {
        inherit system;
        overlays = [
          (import rust-overlay)
          devshell.overlays.default
        ];
      };
      lib = pkgs.lib;

      toolchain_fn = p: p.rust-bin.selectLatestNightlyWith (toolchain: toolchain.default.override {
        extensions = [ "rust-src" "rust-analyzer" ];
      });
      minimal_toolchain_fn = p: p.rust-bin.selectLatestNightlyWith (toolchain: toolchain.minimal);

      craneLib = (crane.mkLib pkgs).overrideToolchain minimal_toolchain_fn;

      common-packages = with pkgs; [
        pkg-config clang mold

        fontconfig
        ( toolchain_fn pkgs )
      ];

      unfilteredRoot = ./.;
      src = lib.fileset.toSource {
        root = unfilteredRoot;
        fileset = lib.fileset.unions [
          (craneLib.fileset.commonCargoSources unfilteredRoot)
          ./crates/dendrite/assets
          (lib.fileset.fileFilter (f: f.hasExt "md") ./vendor/masonry/src)
        ];
      };

      linux-packages = with pkgs; [
        pkg-config

        vulkan-headers vulkan-loader
        vulkan-tools vulkan-tools-lunarg
        vulkan-extension-layer
        # vulkan-validation-layers
      ];

      make-pkg-config-path = packages:
        pkgs.lib.concatStringsSep ":" (
          pkgs.lib.concatMap
            (pkg: map (sub: "${pkgs.lib.getDev pkg}/${sub}") [ "lib/pkgconfig" "share/pkgconfig" ])
            packages
        );

      linux-devshell = pkgs.devshell.mkShell (let
        packages = common-packages ++ linux-packages ++ linux-runtime-libs;
      in {
        inherit packages;
        motd = "\n  Welcome to the {2}$(basename $PRJ_ROOT){reset} shell.\n";
        env = [
          { name = "LD_LIBRARY_PATH"; value = pkgs.lib.makeLibraryPath packages; }
          { name = "PKG_CONFIG_PATH"; value = make-pkg-config-path packages; }
        ];
      });
      # Libraries the binary `dlopen`s at runtime, so the linker never records
      # them; they go on the binary's RPATH instead. winit loads the Wayland
      # and X11 client libraries, wgpu loads Vulkan or (as a fallback) EGL/GL,
      # and fontique loads fontconfig to find system fonts for fallback.
      linux-runtime-libs = with pkgs; [
        wayland libxkbcommon
        libx11 libxcb libxcursor libxi
        vulkan-loader libglvnd
        fontconfig
      ];

      meta = craneLib.crateNameFromCargoToml { cargoToml = ./Cargo.toml; };
      common-args = {
        pname = "dendrite";
        inherit (meta) version;
        inherit src;
        strictDeps = true;
        nativeBuildInputs = with pkgs; [ pkg-config ];
        buildInputs = lib.optionals pkgs.stdenv.isDarwin [ pkgs.apple-sdk ];
      };

      # `buildDepsOnly` stubs out every path crate, but the patched masonry
      # is a real dependency, so its sources go back in.
      cargoArtifacts = craneLib.buildDepsOnly (common-args // {
        dummySrc = craneLib.mkDummySrc {
          inherit src;
          extraDummyScript = ''
            rm -rf $out/vendor/masonry
            cp -r --no-preserve=mode,ownership ${src}/vendor/masonry $out/vendor/masonry
          '';
        };
      });

      dendrite = craneLib.buildPackage (common-args // {
        inherit cargoArtifacts;
        pname = "dendrite";
        cargoExtraArgs = "--locked -p dendrite";
        postInstall = lib.optionalString pkgs.stdenv.isLinux ''
          install -Dm644 ${./packaging/linux/sh.jlewis.Dendrite.desktop} \
            $out/share/applications/sh.jlewis.Dendrite.desktop
        '';
        postFixup = lib.optionalString pkgs.stdenv.isLinux ''
          patchelf --add-rpath ${lib.makeLibraryPath linux-runtime-libs} $out/bin/dendrite
        '';
        meta.mainProgram = "dendrite";
      });

      darwin-devshell = pkgs.mkShell (let
        packages = common-packages ++ [ pkgs.apple-sdk ];
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
      devShell = if pkgs.stdenv.isLinux then linux-devshell else darwin-devshell;
  });
}

