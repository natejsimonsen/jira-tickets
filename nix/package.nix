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
    fileset = lib.fileset.unions [ ../Cargo.toml ../Cargo.lock ../README.md ../macos ../prompts ../src ../tests ];
  };
  cargoLock.lockFile = ../Cargo.lock;
  # On macOS, ship an app bundle so launchers and Finder see it; bin/ links to the binary inside.
  postInstall = lib.optionalString stdenv.hostPlatform.isDarwin ''
    app="$out/Applications/Jira Tickets.app"
    mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
    substitute macos/Info.plist "$app/Contents/Info.plist" --replace-fail @version@ ${manifest.version}
    mv $out/bin/jira-tickets "$app/Contents/MacOS/jira-tickets"
    ln -s "$app/Contents/MacOS/jira-tickets" $out/bin/jira-tickets
  '';
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
