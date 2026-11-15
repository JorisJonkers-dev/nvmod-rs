# Build and run environment of the GUI on NixOS (`nix-shell gui/nix/shell.nix`, then `cargo build` inside gui/).
# GPUI opens the Wayland client, xkbcommon and the Vulkan loader with dlopen at run time, so they must be on the
# loader path or the window fails to start ("the wayland library could not be loaded").
{ pkgs ? import <nixpkgs> { } }:
let
  runtimeLibs = import ./runtime-libs.nix { inherit pkgs; };
in
pkgs.mkShell {
  nativeBuildInputs = with pkgs; [ gcc binutils pkg-config ];
  buildInputs = with pkgs; [ fontconfig freetype libx11 libxcb openssl ] ++ runtimeLibs;
  LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath runtimeLibs;
  # The lld that rustc picks by default does not find these; bfd does.
  RUSTFLAGS = "-C link-arg=-fuse-ld=bfd";
}
