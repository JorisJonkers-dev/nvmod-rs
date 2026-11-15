# Wraps a built `nvmod-gui` binary so it finds its run-time libraries on NixOS:
#   nix-build gui/nix/wrapper.nix --argstr binary "$PWD/gui/target/release/nvmod-gui"
# The result is `result/bin/nvmod-gui`, which sets LD_LIBRARY_PATH and then runs the binary.
{ pkgs ? import <nixpkgs> { }, binary }:
let
  runtimeLibs = import ./runtime-libs.nix { inherit pkgs; };
in
pkgs.runCommand "nvmod-gui-wrapped" { nativeBuildInputs = [ pkgs.makeWrapper ]; } ''
  mkdir -p $out/bin
  makeWrapper ${binary} $out/bin/nvmod-gui \
    --prefix LD_LIBRARY_PATH : ${pkgs.lib.makeLibraryPath runtimeLibs}
''
