#!/usr/bin/env bash
# Gera os instaladores em dist/ com o empacotador do Tauri.
#   Linux : AppImage (WebKitGTK embutido — roda em qualquer distro) + .deb + binário .tar.gz
#   macOS : .app + .dmg (binário universal Intel + Apple Silicon)
set -euo pipefail
. "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
cd "$ROOT"
command -v cargo-tauri >/dev/null 2>&1 || scripts/setup.sh
. scripts/lib.sh

VERSION="$(grep -m1 '^version = ' Cargo.toml | cut -d'"' -f2)"
export AVDMENU_VERSION="$VERSION"
rm -rf dist && mkdir -p dist

case "$(uname -s)" in
Linux)
    export NO_STRIP=true            # linuxdeploy antigo não entende seções .relr de binários novos
    export APPIMAGE_EXTRACT_AND_RUN=1   # não exige FUSE na máquina de build
    cargo tauri build --bundles appimage,deb
    B="target/release/bundle"
    cp "$B"/appimage/*.AppImage "dist/AVD-Menu-v${VERSION}-linux-x86_64.AppImage"
    cp "$B"/deb/*.deb "dist/AVD-Menu-v${VERSION}-linux-amd64.deb"
    # binário solto (usa o WebKitGTK do sistema) + CLI sem interface gráfica
    cargo build --release -p avdcore --bin avd-menu-cli
    tar -czf "dist/avd-menu-v${VERSION}-linux-x86_64.tar.gz" -C target/release avd-menu \
        --transform 's|^avd-menu$|avd-menu/avd-menu|' --transform 's|^|./|' 2>/dev/null || \
        tar -czf "dist/avd-menu-v${VERSION}-linux-x86_64.tar.gz" -C target/release avd-menu
    ;;
Darwin)
    ensure_rustup
    rustup target add aarch64-apple-darwin x86_64-apple-darwin
    cargo tauri build --target universal-apple-darwin --bundles app,dmg
    B="target/universal-apple-darwin/release/bundle"
    cp "$B"/dmg/*.dmg "dist/AVD-Menu-v${VERSION}-macos.dmg"
    ditto -c -k --keepParent "$B/macos/AVD Menu.app" "dist/AVD-Menu-v${VERSION}-macos.zip"
    ;;
*) die "sistema não suportado: $(uname -s)" ;;
esac

(cd dist && if command -v sha256sum >/dev/null 2>&1; then sha256sum *; else shasum -a 256 *; fi > SHA256SUMS)
say "pronto:"; ls -la dist
