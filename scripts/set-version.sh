#!/usr/bin/env bash
# Define a versão do app (Cargo.toml do workspace e tauri.conf.json): set-version.sh 1.2.3
set -euo pipefail
. "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
v="${1:?uso: set-version.sh VERSÃO (ex.: 1.2.3 ou v1.2.3)}"
v="${v#v}"
[[ "$v" =~ ^[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.-]+)?$ ]] || die "versão inválida: $v"
sed -i.bak -E "0,/^version = \".*\"/s//version = \"$v\"/" "$ROOT/Cargo.toml"
sed -i.bak -E "s/^(  \"version\": )\".*\"/\1\"$v\"/" "$ROOT/src-tauri/tauri.conf.json"
rm -f "$ROOT/Cargo.toml.bak" "$ROOT/src-tauri/tauri.conf.json.bak"
say "versão $v"
