#!/usr/bin/env bash
# Gera dist/Flounder-<versão>-x86_64.AppImage a partir do build de release.
# Requer o appimagetool (https://github.com/AppImage/appimagetool/releases, 1.9.1) no PATH
# ou em APPIMAGETOOL. A AppImage herda a glibc da máquina de build: gere numa distro antiga
# (ou no CI, Ubuntu 22.04) para rodar em mais sistemas.
set -euo pipefail
cd "$(dirname "$0")/.."
tool=${APPIMAGETOOL:-appimagetool}
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
cargo build --release --locked
dir=dist/Flounder.AppDir
rm -rf "$dir" && mkdir -p "$dir/usr/bin"
cp target/release/flounder "$dir/usr/bin/"
ln -s usr/bin/flounder "$dir/AppRun"
cp assets/art/tropical_fish.png "$dir/flounder.png"
cat > "$dir/flounder.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=Flounder
Comment=Finanças em casal
Exec=flounder
Icon=flounder
Categories=Office;Finance;
StartupWMClass=flounder
EOF
out="dist/Flounder-$version-x86_64.AppImage"
# Sem FUSE na máquina de build, o próprio appimagetool roda extraído.
APPIMAGE_EXTRACT_AND_RUN=1 ARCH=x86_64 "$tool" --no-appstream "$dir" "$out" >/dev/null
echo "$out"
