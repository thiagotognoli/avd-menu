#!/usr/bin/env bash
# Ambiente comum dos scripts: tudo (Rust, cargo, bibliotecas de desenvolvimento do
# WebKitGTK) fica dentro de .tools/ no próprio projeto — nada é instalado no sistema.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export RUSTUP_HOME="${RUSTUP_HOME:-$ROOT/.tools/rustup}"
export CARGO_HOME="${CARGO_HOME:-$ROOT/.tools/cargo}"
export PATH="$CARGO_HOME/bin:$PATH"

# Bibliotecas de desenvolvimento baixadas por scripts/linux-devlibs.sh (se existirem).
if [ -d "$ROOT/.tools/sysroot/usr/lib/x86_64-linux-gnu/pkgconfig" ]; then
    export PKG_CONFIG_PATH="$ROOT/.tools/sysroot/usr/lib/x86_64-linux-gnu/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
fi

say() { printf '\033[1m==>\033[0m %s\n' "$*"; }
die() { printf 'erro: %s\n' "$*" >&2; exit 1; }
