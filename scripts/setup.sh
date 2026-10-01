#!/usr/bin/env bash
# Prepara o ambiente de desenvolvimento SEM sudo e sem tocar no sistema:
#   - Rust (rustup) em .tools/
#   - cargo-tauri (empacotador) em .tools/
#   - Linux: bibliotecas de desenvolvimento do WebKitGTK, se faltarem (scripts/linux-devlibs.sh)
set -euo pipefail
. "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
cd "$ROOT"

if ! command -v rustc >/dev/null 2>&1; then
    say "Instalando o Rust em .tools/ …"
    mkdir -p .tools
    curl -fsSL https://sh.rustup.rs -o .tools/rustup-init.sh
    sh .tools/rustup-init.sh -y --no-modify-path --profile minimal --default-toolchain stable
fi
say "$(rustc --version)"

if [ "$(uname -s)" = "Linux" ]; then
    "$ROOT/scripts/linux-devlibs.sh"
    . "$ROOT/scripts/lib.sh"
fi

if ! command -v cargo-tauri >/dev/null 2>&1; then
    say "Instalando o cargo-tauri em .tools/ (uns 2 minutos) …"
    cargo install tauri-cli --version "^2" --locked
fi
say "pronto — use ./x build | run | test | dist"
