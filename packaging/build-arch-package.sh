#!/usr/bin/env bash
set -euo pipefail

version="${1:?usage: build-arch-package.sh VERSION OUTPUT_DIR}"
output_dir="${2:?usage: build-arch-package.sh VERSION OUTPUT_DIR}"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary="$root/target/release/appimanager"
locale_file="$(find "$root/target/release/build" -type f \
    -path '*/out/locale/zh_CN/LC_MESSAGES/appimanager.mo' -print -quit)"
work_dir="$(mktemp -d)"
stage="$work_dir/arch-package"

cleanup() {
    rm -rf "$work_dir"
}
trap cleanup EXIT

test -x "$binary"
if [[ -z "$locale_file" ]]; then
    echo "Could not find compiled zh_CN gettext catalog" >&2
    exit 1
fi
mkdir -p "$stage" "$output_dir"
sed "s/@PKGVER@/${version}/g" "$root/packaging/arch/PKGBUILD" > "$stage/PKGBUILD"
install -m 0755 "$binary" "$stage/appimanager"
install -m 0644 "$root/data/io.github.appimanager.desktop" "$stage/io.github.appimanager.desktop"
install -m 0644 "$root/data/io.github.appimanager.gschema.xml" "$stage/io.github.appimanager.gschema.xml"
install -m 0644 "$root/data/icons/hicolor/scalable/apps/io.github.appimanager.svg" "$stage/io.github.appimanager.svg"
install -m 0644 "$locale_file" "$stage/appimanager.mo"
install -m 0644 "$root/LICENSE" "$stage/LICENSE"
install -m 0644 "$root/packaging/arch/appimanager.install" "$stage/appimanager.install"

# Build a normal pacman package with Arch's own makepkg, while reusing the
# release binary compiled on the native Ubuntu x86_64 runner.
host_uid="$(id -u)"
host_gid="$(id -g)"
docker run --rm --platform linux/amd64 \
    --env "HOST_UID=$host_uid" \
    --env "HOST_GID=$host_gid" \
    -v "$stage:/build" \
    -w /build \
    archlinux:base-devel \
    bash -euc '
        useradd --create-home package-builder
        chown -R package-builder:package-builder /build
        trap '\''chown -R "$HOST_UID:$HOST_GID" /build'\'' EXIT
        runuser -u package-builder -- makepkg --noconfirm --nodeps --cleanbuild
    '

package_file="$(find "$stage" -maxdepth 1 -type f -name "appimanager-${version}-1-x86_64.pkg.tar.zst" -print -quit)"
if [[ -z "$package_file" ]]; then
    echo "makepkg did not create the expected Arch package" >&2
    exit 1
fi
cp "$package_file" "$output_dir/"
