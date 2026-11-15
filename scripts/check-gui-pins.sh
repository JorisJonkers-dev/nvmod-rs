#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
# The GPUI crates must all sit at one exact version: every gpui-pre* dependency in gui/Cargo.toml is pinned with
# `=`, the pins agree, and gui/Cargo.lock holds exactly one version of each gpui-pre* package. Also: nothing of
# the GUI toolkits may be in the main workspace's Cargo.lock.
set -euo pipefail
cd "$(dirname "$0")/.."

pins="$(grep -E '^(gpui|gpui_platform) = ' gui/Cargo.toml | sed -E 's/.*version = "=([^"]+)".*/\1/' | sort -u)"
if [ "$(wc -l <<<"$pins")" -ne 1 ] || ! grep -qE '^[0-9]+\.[0-9]+\.[0-9]+$' <<<"$pins"; then
  echo "gui/Cargo.toml: gpui-pre and gpui-pre-platform must both be pinned with an exact =x.y.z that agrees; got: $pins" >&2
  exit 1
fi
grep -qE '^gpui-kit = \{ version = "=[0-9]+\.[0-9]+\.[0-9]+"' gui/Cargo.toml \
  || { echo "gui/Cargo.toml: gpui-kit must be pinned with an exact =x.y.z" >&2; exit 1; }

for pkg in $(grep -oE '^name = "gpui-pre[^"]*"' gui/Cargo.lock | cut -d'"' -f2 | sort -u); do
  versions="$(awk -v p="$pkg" '$0=="name = \"" p "\""{getline; print $3}' gui/Cargo.lock | tr -d '"' | sort -u)"
  if [ "$versions" != "$pins" ]; then
    echo "gui/Cargo.lock: $pkg is at '$versions', expected $pins" >&2
    exit 1
  fi
done

if grep -qE '^name = "(gpui|eframe|egui)' Cargo.lock; then
  echo "Cargo.lock (main workspace) contains a GUI toolkit crate; it must stay in gui/Cargo.lock" >&2
  exit 1
fi
echo "gpui pins agree at $pins"
