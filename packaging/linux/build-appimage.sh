#!/usr/bin/env bash
# Packs an already built release binary into SurvivorsForge-<version>-x86_64.AppImage.
# Usage: packaging/linux/build-appimage.sh <binary> <version> <output dir>
# Nothing is bundled besides the binary: it only links glibc and fontconfig/freetype, which every
# desktop (SteamOS included) has; window and GL libraries are loaded by winit at runtime.
set -euo pipefail

binary="$1"
version="$2"
out="$3"
root="$(cd "$(dirname "$0")/../.." && pwd)"
tool_version="1.9.1"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

app="$work/AppDir"
mkdir -p "$app/usr/bin" "$app/usr/share/icons/hicolor/256x256/apps" "$app/usr/share/applications"
install -m 755 "$binary" "$app/usr/bin/SurvivorsForge"
install -m 755 "$root/packaging/linux/AppRun" "$app/AppRun"
install -m 644 "$root/assets/survivorsforge.desktop" "$app/survivorsforge.desktop"
install -m 644 "$root/assets/survivorsforge.desktop" "$app/usr/share/applications/survivorsforge.desktop"
install -m 644 "$root/assets/icon.png" "$app/survivorsforge.png"
install -m 644 "$root/assets/icon.png" "$app/usr/share/icons/hicolor/256x256/apps/survivorsforge.png"

tool="$work/appimagetool"
curl -sSfL -o "$tool" "https://github.com/AppImage/appimagetool/releases/download/${tool_version}/appimagetool-x86_64.AppImage"
chmod +x "$tool"

mkdir -p "$out"
# Containers and CI runners usually have no FUSE: let the tool unpack itself instead of mounting.
ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 "$tool" --no-appstream "$app" "$out/SurvivorsForge-${version}-x86_64.AppImage"
