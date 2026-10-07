{ lib, stdenv, mkShell, cargo, rustc, clippy, rustfmt, rust-analyzer, libxkbcommon, wayland, libGL, vulkan-loader, xorg }:
let
  runtimeLibs = [ libxkbcommon wayland libGL vulkan-loader xorg.libX11 xorg.libXcursor xorg.libXi xorg.libXrandr xorg.libxcb ];
in
mkShell {
  packages = [ cargo rustc clippy rustfmt rust-analyzer ];
  LD_LIBRARY_PATH = lib.optionalString stdenv.hostPlatform.isLinux (lib.makeLibraryPath runtimeLibs);
}
