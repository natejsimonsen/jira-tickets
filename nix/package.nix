{ lib, stdenv, rustPlatform, libxkbcommon, wayland, libGL, vulkan-loader, xorg }:
let
  manifest = (lib.importTOML ../Cargo.toml).package;
  runtimeLibs = [ libxkbcommon wayland libGL vulkan-loader xorg.libX11 xorg.libXcursor xorg.libXi xorg.libXrandr xorg.libxcb ];
in
rustPlatform.buildRustPackage {
  pname = manifest.name;
  version = manifest.version;
  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [ ../Cargo.toml ../Cargo.lock ../src ];
  };
  cargoLock.lockFile = ../Cargo.lock;
  postFixup = lib.optionalString stdenv.hostPlatform.isLinux ''
    patchelf --add-rpath ${lib.makeLibraryPath runtimeLibs} $out/bin/jira-tickets
  '';
  meta = {
    description = manifest.description;
    license = lib.licenses.mit;
    mainProgram = "jira-tickets";
    platforms = lib.platforms.darwin ++ lib.platforms.linux;
  };
}
