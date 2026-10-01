#!/usr/bin/env bash
# Tarefas do projeto (tudo em bash/Rust):
#   ./x setup          prepara o ambiente dentro do projeto (Rust, cargo-tauri, libs; sem sudo)
#   ./x build          compila o app (release) em target/release/avd-menu
#   ./x run [args]     compila e abre o app (modo debug); com args roda a linha de comando
#   ./x ui [porta]     serve só a interface no navegador (devserver) para desenvolver/testar o front
#   ./x test           cargo fmt --check, clippy e todos os testes
#   ./x dist [versão]  gera os instaladores em dist/ (AppImage + .deb no Linux; .dmg no macOS)
#   ./x gen-devices    regenera a lista de aparelhos (precisa de um SDK com cmdline-tools e JDK)
#   ./x icons          regenera os ícones do app a partir de crates/core/assets/icon-1024.png
#   ./x clean
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
. scripts/lib.sh

need_toolchain() {
    command -v cargo >/dev/null 2>&1 || { say "Rust não encontrado — preparando o ambiente"; scripts/setup.sh; . scripts/lib.sh; }
}

cmd="${1:-help}"; shift || true
case "$cmd" in
    setup) scripts/setup.sh ;;
    build) need_toolchain; cargo build --release -p avd-menu ;;
    run) need_toolchain; cargo run -p avd-menu -- "$@" ;;
    ui)
        need_toolchain
        port="${1:-18080}"
        say "interface em http://127.0.0.1:$port/  (API real do núcleo; Ctrl+C para sair)"
        cargo run -q -p avdcore --features devserver --bin devserver -- --port "$port" --ui ui
        ;;
    test)
        need_toolchain
        cargo fmt --all -- --check
        cargo clippy --workspace --all-targets -- -D warnings
        cargo test --workspace
        ;;
    dist)
        need_toolchain
        [ -n "${1:-}" ] && scripts/set-version.sh "$1"
        scripts/dist.sh
        ;;
    gen-devices) need_toolchain; cargo xtask gen-devices "$@" ;;
    icons) need_toolchain; cargo tauri icon crates/core/assets/icon-1024.png -o src-tauri/icons
           rm -rf src-tauri/icons/android src-tauri/icons/ios src-tauri/icons/Square*.png src-tauri/icons/StoreLogo.png ;;
    clean) cargo clean; rm -rf dist ;;
    *) sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//' ;;
esac
