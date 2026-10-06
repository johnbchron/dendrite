# portable-cc: a C toolchain from an older nixpkgs. When set, link against its
# glibc and point the binary at the FHS loader so it runs outside Nix.
{ pkgs, lib, crane, portable-cc ? null, ... }: let
  minimal_toolchain_fn = p: p.rust-bin.stable.latest.minimal;

  portable = portable-cc != null;

  craneLib = let
    base = (crane.mkLib pkgs).overrideToolchain minimal_toolchain_fn;
  in if portable
    then base.overrideScope (_: _: { stdenvSelector = p: p.overrideCC p.stdenv portable-cc; })
    else base;

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
    pname = if portable then "dendrite-portable" else "dendrite";
    inherit (meta) version;
    inherit src;
    strictDeps = true;
    nativeBuildInputs = with pkgs; [ pkg-config ];
    buildInputs = lib.optionals pkgs.stdenv.hostPlatform.isDarwin [ pkgs.apple-sdk ];
  } // lib.optionalAttrs portable {
    # the rust toolchain propagates the new cc onto PATH, so name the old one
    "CARGO_TARGET_${pkgs.stdenv.hostPlatform.rust.cargoEnvVarTarget}_LINKER" = "${portable-cc}/bin/cc";
    CC = "${portable-cc}/bin/cc";
  };

  fhs-interpreter = {
    x86_64-linux = "/lib64/ld-linux-x86-64.so.2";
    aarch64-linux = "/lib/ld-linux-aarch64.so.1";
  }.${pkgs.stdenv.hostPlatform.system};

  # swap in the system loader, then fail if any symbol needs a newer glibc
  portable-fixup = ''
    patchelf --set-interpreter ${fhs-interpreter} --remove-rpath $out/bin/dendrite
    # patchelf leaves the old loader and RUNPATH strings behind, unused
    ${pkgs.removeReferencesTo}/bin/remove-references-to -t ${portable-cc.libc} $out/bin/dendrite
    newest=$(${pkgs.binutils-unwrapped}/bin/objdump -T $out/bin/dendrite \
      | grep -o 'GLIBC_[0-9.]*' | sed 's/GLIBC_//' | sort -uV | tail -1)
    if [ "$(printf '%s\n' "$newest" ${portable-cc.libc.version} | sort -V | tail -1)" != ${portable-cc.libc.version} ]; then
      echo "dendrite needs GLIBC_$newest, newer than ${portable-cc.libc.version}" >&2
      exit 1
    fi
  '';

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
    cargoExtraArgs = "--locked -p dendrite";
    postInstall = lib.optionalString pkgs.stdenv.hostPlatform.isLinux ''
      install -Dm644 ${./packaging/linux/sh.jlewis.Dendrite.desktop} \
        $out/share/applications/sh.jlewis.Dendrite.desktop
    '';
    postFixup = if portable then portable-fixup else lib.optionalString pkgs.stdenv.hostPlatform.isLinux ''
      patchelf --add-rpath ${lib.makeLibraryPath linux-runtime-libs} $out/bin/dendrite
    '';
    meta.mainProgram = "dendrite";
  } // lib.optionalAttrs portable {
    # a portable build must not reach into /nix/store
    allowedReferences = [ ];
  });
in dendrite
