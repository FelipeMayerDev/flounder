#!/usr/bin/env bash
# Gera dist/flounder-<versão>-linux-x86_64.tar.gz a partir do build de release.
set -euo pipefail
cd "$(dirname "$0")/.."
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
cargo build --release --locked
name="flounder-$version-linux-x86_64"
rm -rf "dist/$name" && mkdir -p "dist/$name"
cp target/release/flounder "dist/$name/"
cp docs/EXECUTAR.txt "dist/$name/LEIA-ME.txt"
cp assets/fonts/LICENSE.txt "dist/$name/LICENSE-Inter.txt"
cp assets/art/LICENSE.txt "dist/$name/LICENSE-FluentEmoji.txt"
cp assets/fonts/LICENSE-NotoEmoji.txt "dist/$name/LICENSE-NotoEmoji.txt"
tar -C dist -czf "dist/$name.tar.gz" "$name"
echo "dist/$name.tar.gz"
