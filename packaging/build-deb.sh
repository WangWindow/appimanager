#!/usr/bin/env bash
set -euo pipefail

version="${1:?usage: build-deb.sh VERSION ARCH OUTPUT_DIR}"
arch="${2:?usage: build-deb.sh VERSION ARCH OUTPUT_DIR}"
output_dir="${3:?usage: build-deb.sh VERSION ARCH OUTPUT_DIR}"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary="$root/target/release/appimanager"
work_dir="$(mktemp -d)"
package_root="$work_dir/package"

cleanup() {
    rm -rf "$work_dir"
}
trap cleanup EXIT

test -x "$binary"
mkdir -p \
    "$package_root/DEBIAN" \
    "$package_root/usr/bin" \
    "$package_root/usr/share/applications" \
    "$package_root/usr/share/icons/hicolor/scalable/apps" \
    "$package_root/usr/share/glib-2.0/schemas" \
    "$package_root/usr/share/locale/zh_CN/LC_MESSAGES" \
    "$package_root/usr/share/doc/appimanager"
install -m 0755 "$binary" "$package_root/usr/bin/appimanager"
install -m 0644 "$root/data/io.github.appimanager.desktop" \
    "$package_root/usr/share/applications/io.github.appimanager.desktop"
install -m 0644 "$root/data/icons/hicolor/scalable/apps/io.github.appimanager.svg" \
    "$package_root/usr/share/icons/hicolor/scalable/apps/io.github.appimanager.svg"
install -m 0644 "$root/data/io.github.appimanager.gschema.xml" \
    "$package_root/usr/share/glib-2.0/schemas/io.github.appimanager.gschema.xml"
locale_file="$(find "$root/target/release/build" -type f \
    -path '*/out/locale/zh_CN/LC_MESSAGES/appimanager.mo' -print -quit)"
if [[ -z "$locale_file" ]]; then
    echo "Could not find compiled zh_CN gettext catalog" >&2
    exit 1
fi
install -m 0644 "$locale_file" \
    "$package_root/usr/share/locale/zh_CN/LC_MESSAGES/appimanager.mo"
install -m 0644 "$root/LICENSE" "$package_root/usr/share/doc/appimanager/LICENSE"
install -m 0755 "$root/packaging/deb/postinst" "$package_root/DEBIAN/postinst"
install -m 0755 "$root/packaging/deb/postrm" "$package_root/DEBIAN/postrm"

# Resolve Depends from the actual ELF links on each native build architecture.
mkdir -p "$work_dir/debian"
cat > "$work_dir/debian/control" <<'CONTROL'
Source: appimanager
Section: utils
Priority: optional
Maintainer: AppImage Manager contributors <appimanager@users.noreply.github.com>
Standards-Version: 4.6.2

Package: appimanager
Architecture: any
Depends: ${shlibs:Depends}, desktop-file-utils, libglib2.0-bin, libgtk-4-bin
Description: Local AppImage integration manager
 Manage locally installed AppImage applications with a GTK interface.
CONTROL
shlib_output="$(cd "$work_dir" && dpkg-shlibdeps -O -e "$package_root/usr/bin/appimanager")"
dependencies="${shlib_output#shlibs:Depends=}"
if [[ -z "$dependencies" || "$dependencies" == "$shlib_output" ]]; then
    echo "dpkg-shlibdeps did not produce runtime dependencies" >&2
    exit 1
fi

cat > "$package_root/DEBIAN/control" <<CONTROL
Package: appimanager
Version: ${version}-1
Section: utils
Priority: optional
Architecture: ${arch}
Maintainer: AppImage Manager contributors <appimanager@users.noreply.github.com>
Depends: ${dependencies}, desktop-file-utils, libglib2.0-bin, libgtk-4-bin
Description: Local AppImage integration manager
 Manage locally installed AppImage applications with a GTK interface.
CONTROL

mkdir -p "$output_dir"
dpkg-deb --build --root-owner-group \
    "$package_root" \
    "$output_dir/appimanager_${version}_${arch}.deb"
dpkg-deb --info "$output_dir/appimanager_${version}_${arch}.deb"
