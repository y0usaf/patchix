{
  rustPlatform,
  lib,
}: let
  fs = lib.fileset;
in
  rustPlatform.buildRustPackage (finalAttrs: {
    pname = "patchix";
    version = (builtins.fromTOML (builtins.readFile "${finalAttrs.src}/Cargo.toml")).package.version;
    src = fs.toSource {
      root = ../.;
      fileset = fs.unions [
        (fs.fileFilter (file: builtins.any file.hasExt ["rs"]) ../src)
        (fs.fileFilter (file: builtins.any file.hasExt ["rs"]) ../tests)
        ../Cargo.lock
        ../Cargo.toml
      ];
    };
    cargoLock.lockFile = ../Cargo.lock;
    meta.mainProgram = "patchix";
  })
