# The libraries the GUI binary loads at run time (dlopen), shared by shell.nix and wrapper.nix.
{ pkgs }:
with pkgs; [ wayland libxkbcommon vulkan-loader libGL libx11 libxcb libxkbcommon fontconfig freetype ]
