#!/usr/bin/env bash
set -euo pipefail

version="${1:?usage: build-appimage.sh VERSION ARCH OUTPUT_DIR}"
arch="${2:?usage: build-appimage.sh VERSION ARCH OUTPUT_DIR}"
output_dir="${3:?usage: build-appimage.sh VERSION ARCH OUTPUT_DIR}"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary="$root/target/release/appimanager"
work_dir="$(mktemp -d)"
appdir="$work_dir/AppDir"
tools="$work_dir/tools"
app_name="AppImageManager-${version}-${arch}.AppImage"

cleanup() {
    rm -rf "$work_dir"
}
trap cleanup EXIT

test -x "$binary"
mkdir -p \
    "$tools" \
    "$appdir/usr/bin" \
    "$appdir/usr/share/applications" \
    "$appdir/usr/share/glib-2.0/schemas" \
    "$appdir/usr/share/locale/zh_CN/LC_MESSAGES" \
    "$appdir/usr/share/doc/appimanager" \
    "$appdir/usr/share/icons/hicolor/scalable/apps" \
    "$output_dir"
output_dir="$(cd "$output_dir" && pwd)"
appimage_output="$output_dir/$app_name"
install -m 0755 "$binary" "$appdir/usr/bin/appimanager"
sed \
    -e 's/^Exec=appimanager %U$/Exec=AppRun %U/' \
    -e 's/^Icon=.*/Icon=io.github.appimanager/' \
    "$root/data/io.github.appimanager.desktop" \
    > "$appdir/usr/share/applications/io.github.appimanager.desktop"
install -m 0644 "$root/data/io.github.appimanager.gschema.xml" \
    "$appdir/usr/share/glib-2.0/schemas/io.github.appimanager.gschema.xml"
locale_file="$(find "$root/target/release/build" -type f \
    -path '*/out/locale/zh_CN/LC_MESSAGES/appimanager.mo' -print -quit)"
if [[ -z "$locale_file" ]]; then
    echo "Could not find compiled zh_CN gettext catalog" >&2
    exit 1
fi
install -m 0644 "$locale_file" \
    "$appdir/usr/share/locale/zh_CN/LC_MESSAGES/appimanager.mo"
install -m 0644 "$root/COPYING" "$appdir/usr/share/doc/appimanager/COPYING"
install -m 0755 "$root/packaging/AppRun" "$appdir/AppRun"
glib-compile-schemas "$appdir/usr/share/glib-2.0/schemas"

# Use the host GTK theme's generic executable icon for AppImage file managers.
icon_source="$(find /usr/share/icons -type f \
    -name 'application-x-executable-symbolic.svg' -print -quit)"
if [[ -z "$icon_source" ]]; then
    echo "Could not find the GTK executable symbolic icon" >&2
    exit 1
fi
install -m 0644 "$icon_source" \
    "$appdir/usr/share/icons/hicolor/scalable/apps/io.github.appimanager.svg"
install -m 0644 "$icon_source" "$appdir/.DirIcon"

case "$arch" in
    x86_64|aarch64) ;;
    *) echo "Unsupported AppImage architecture: $arch" >&2; exit 1 ;;
esac
linuxdeploy="$tools/linuxdeploy-${arch}.AppImage"
curl --fail --location --silent --show-error \
    "https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-${arch}.AppImage" \
    --output "$linuxdeploy"
curl --fail --location --silent --show-error \
    "https://raw.githubusercontent.com/linuxdeploy/linuxdeploy-plugin-gtk/master/linuxdeploy-plugin-gtk.sh" \
    --output "$tools/linuxdeploy-plugin-gtk.sh"
chmod 0755 "$linuxdeploy" "$tools/linuxdeploy-plugin-gtk.sh"

# linuxdeploy-plugin-gtk attempts to copy GTK4's module directory even when
# the distro ships no optional GTK4 modules and therefore omits the directory.
gtk4_module_dir="$(pkg-config --variable=libdir gtk4)/gtk-4.0"
sudo mkdir -p "$gtk4_module_dir"

export APPIMAGE_EXTRACT_AND_RUN=1
export ARCH="$arch"
export LDAI_NO_APPSTREAM=1
export LDAI_OUTPUT="$appimage_output"
export LDAI_VERSION="$version"
"$linuxdeploy" \
    --appdir "$appdir" \
    --executable "$appdir/usr/bin/appimanager" \
    --desktop-file "$appdir/usr/share/applications/io.github.appimanager.desktop" \
    --icon-file "$appdir/usr/share/icons/hicolor/scalable/apps/io.github.appimanager.svg" \
    --custom-apprun="$root/packaging/AppRun" \
    --plugin gtk \
    --output appimage

test -s "$appimage_output"
chmod 0755 "$appimage_output"
"$appimage_output" --version
