#!/bin/sh
# Instala o AVD Menu a partir da última release do GitHub.
#
#   curl -fsSL https://raw.githubusercontent.com/thiagotognoli/avd-menu/main/install.sh | sh
#
# Linux : baixa o AppImage para ~/.local/bin/avd-menu e adiciona ao menu de aplicativos.
# macOS : baixa o .dmg e copia "AVD Menu.app" para ~/Applications.
#
# Variáveis: AVDMENU_VERSION=v1.0.0 (padrão: última), AVDMENU_REPO=dono/repo
set -eu

REPO="${AVDMENU_REPO:-thiagotognoli/avd-menu}"
VERSION="${AVDMENU_VERSION:-latest}"

say() { printf '%s\n' "$*"; }
die() { printf 'Erro: %s\n' "$*" >&2; exit 1; }

fetch() {
    if command -v curl >/dev/null 2>&1; then curl -fsSL "$1"; else wget -qO- "$1"; fi
}
download() {
    if command -v curl >/dev/null 2>&1; then curl -fL --progress-bar -o "$2" "$1"; else wget -O "$2" "$1"; fi
}

if [ "$VERSION" = "latest" ]; then
    API="https://api.github.com/repos/$REPO/releases/latest"
else
    API="https://api.github.com/repos/$REPO/releases/tags/$VERSION"
fi

say "Consultando a release ($REPO, $VERSION) …"
JSON="$(fetch "$API")" || die "não consegui consultar $API (a release existe?)"

# primeira URL de download que termina com o sufixo pedido
asset_url() {
    printf '%s' "$JSON" | tr ',' '\n' | grep '"browser_download_url"' | grep -e "$1\"" | head -1 | sed 's/.*"browser_download_url": *"\([^"]*\)".*/\1/'
}

case "$(uname -s)" in
Linux)
    case "$(uname -m)" in
        x86_64|amd64) ;;
        *) die "o Android Emulator do Google só existe para Linux x86_64 (esta máquina é $(uname -m))." ;;
    esac
    URL="$(asset_url 'x86_64.AppImage')"
    [ -n "$URL" ] || die "AppImage não encontrado nessa release."
    DEST="${AVDMENU_BIN_DIR:-$HOME/.local/bin}"
    mkdir -p "$DEST"
    say "Baixando $(basename "$URL") …"
    download "$URL" "$DEST/avd-menu.tmp"
    chmod +x "$DEST/avd-menu.tmp"
    mv -f "$DEST/avd-menu.tmp" "$DEST/avd-menu"
    say "Instalado em $DEST/avd-menu"
    if ! "$DEST/avd-menu" version >/dev/null 2>&1; then
        say "Aviso: o AppImage não abriu — falta o FUSE? Instale 'libfuse2' (ou 'fuse') ou rode com:"
        say "  $DEST/avd-menu --appimage-extract-and-run"
    else
        "$DEST/avd-menu" install || true
    fi
    case ":$PATH:" in *":$DEST:"*) ;; *) say "Dica: adicione $DEST ao seu PATH." ;; esac
    say "Pronto! Procure por \"AVD Menu\" no menu de aplicativos, ou rode: avd-menu"
    ;;
Darwin)
    URL="$(asset_url '-macos.dmg')"
    [ -n "$URL" ] || die "DMG não encontrado nessa release."
    TMP="$(mktemp -d)"
    trap 'hdiutil detach "$TMP/mnt" -quiet >/dev/null 2>&1 || true; rm -rf "$TMP"' EXIT
    say "Baixando $(basename "$URL") …"
    download "$URL" "$TMP/avd-menu.dmg"
    mkdir -p "$TMP/mnt"
    hdiutil attach "$TMP/avd-menu.dmg" -nobrowse -quiet -mountpoint "$TMP/mnt"
    mkdir -p "$HOME/Applications"
    rm -rf "$HOME/Applications/AVD Menu.app"
    cp -R "$TMP/mnt/AVD Menu.app" "$HOME/Applications/"
    # o app não é notarizado: tira a quarentena para abrir sem o aviso do Gatekeeper
    xattr -dr com.apple.quarantine "$HOME/Applications/AVD Menu.app" 2>/dev/null || true
    say "Instalado em ~/Applications/AVD Menu.app"
    ;;
*)
    die "sistema não suportado: $(uname -s)"
    ;;
esac
