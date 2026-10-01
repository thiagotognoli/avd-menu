#!/usr/bin/env bash
# Linux (Debian/Ubuntu): se faltarem os cabeçalhos do WebKitGTK/GTK/libsoup, baixa os
# pacotes -dev (sem sudo, com `apt-get download`) e os extrai em .tools/sysroot, com
# os .pc ajustados. Só é preciso para COMPILAR; o app em si usa as bibliotecas
# normais do sistema (ou as embutidas no AppImage).
set -euo pipefail
. "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
cd "$ROOT"

have() { pkg-config --exists "$1" 2>/dev/null; }

if have webkit2gtk-4.1 && have gtk+-3.0 && have libsoup-3.0; then
    say "bibliotecas de desenvolvimento do WebKitGTK: ok"
    exit 0
fi
command -v apt-get >/dev/null 2>&1 || die "faltam webkit2gtk-4.1, gtk+-3.0 e libsoup-3.0 (pacotes -dev); instale com o gerenciador da sua distribuição (veja README)"
command -v pkg-config >/dev/null 2>&1 || die "instale o pkg-config (sudo apt install pkg-config build-essential)"

SYS="$ROOT/.tools/sysroot"
DEB="$ROOT/.tools/debs"
mkdir -p "$SYS" "$DEB"
say "baixando os pacotes -dev do WebKitGTK (sem sudo) …"
pkgs="$(apt-get -s install libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libssl-dev 2>/dev/null | awk '/^Inst/ {print $2}' | tr '\n' ' ')"
[ -n "$pkgs" ] || pkgs="libwebkit2gtk-4.1-dev"
(cd "$DEB" && apt-get download $pkgs >/dev/null)
for d in "$DEB"/*.deb; do dpkg -x "$d" "$SYS"; done

# Aponta os .pc para o sysroot e refaz os links .so (os alvos ficam no sistema).
LIBDIR="$SYS/usr/lib/$(dpkg-architecture -qDEB_HOST_MULTIARCH 2>/dev/null || echo x86_64-linux-gnu)"
for pc in "$LIBDIR"/pkgconfig/*.pc; do
    [ -e "$pc" ] || continue
    sed -i "s#^prefix=/usr#prefix=$SYS/usr#; s#^libdir=/usr/lib/#libdir=$SYS/usr/lib/#; s#^includedir=/usr/include#includedir=$SYS/usr/include#" "$pc"
done
for so in "$LIBDIR"/*.so; do
    [ -L "$so" ] || continue
    ln -sfn "/usr/lib/$(basename "$LIBDIR")/$(readlink "$so")" "$so"
done
say "ok: bibliotecas em .tools/sysroot (usadas automaticamente pelo ./x)"
