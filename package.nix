{ pkgs, lib, crane, ... }: let
  minimal_toolchain_fn = p: p.rust-bin.selectLatestNightlyWith (toolchain: toolchain.minimal);

  craneLib = (crane.mkLib pkgs).overrideToolchain minimal_toolchain_fn;

  unfilteredRoot = ./.;
  src = lib.fileset.toSource {
    root = unfilteredRoot;
    fileset = lib.fileset.unions [
      (craneLib.fileset.commonCargoSources unfilteredRoot)
      ./crates/dendrite/assets
      (lib.fileset.fileFilter (f: f.hasExt "md") ./vendor/masonry/src)
    ];
  };

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
    buildInputs = lib.optionals pkgs.stdenv.hostPlatform.isDarwin [ pkgs.apple-sdk ];
  };

  # patch vendored masonry back in
  cargoArtifacts = craneLib.buildDepsOnly (
    ((builtins.removeAttrs common-args [ "src" ]) // {
      dummySrc = craneLib.mkDummySrc {
        inherit src;
        extraDummyScript = ''
          rm -rf $out/vendor/masonry
          cp -r --no-preserve=mode,ownership ${src}/vendor/masonry $out/vendor/masonry
        '';
      };
    })
  );

  dendrite = craneLib.buildPackage (common-args // {
    inherit cargoArtifacts;
    pname = "dendrite";
    cargoExtraArgs = "--locked -p dendrite";
    postInstall = lib.optionalString pkgs.stdenv.hostPlatform.isLinux ''
      install -Dm644 ${./packaging/linux/sh.jlewis.Dendrite.desktop} \
        $out/share/applications/sh.jlewis.Dendrite.desktop
    '';
    postFixup = lib.optionalString pkgs.stdenv.hostPlatform.isLinux ''
      patchelf --add-rpath ${lib.makeLibraryPath linux-runtime-libs} $out/bin/dendrite
    '';
    meta.mainProgram = "dendrite";
  });
in dendrite
