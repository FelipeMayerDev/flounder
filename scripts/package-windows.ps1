# Gera dist/flounder-<versão>-windows-x86_64.zip (primeira instalação) e o .exe solto
# (o que a atualização automática baixa) a partir do build de release.
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")
cargo build --release --locked
$v = (Select-String -Path Cargo.toml -Pattern '^version = "(.*)"').Matches[0].Groups[1].Value
$n = "flounder-$v-windows-x86_64"
New-Item -ItemType Directory -Force "dist/$n" | Out-Null
Copy-Item target/release/flounder.exe "dist/$n/"
Copy-Item docs/EXECUTAR.txt "dist/$n/LEIA-ME.txt"
Copy-Item assets/fonts/LICENSE.txt "dist/$n/LICENSE-Inter.txt"
Copy-Item assets/art/LICENSE.txt "dist/$n/LICENSE-FluentEmoji.txt"
Compress-Archive -Path "dist/$n" -DestinationPath "dist/$n.zip" -Force
Copy-Item target/release/flounder.exe "dist/$n.exe"
Write-Output "dist/$n.zip"
Write-Output "dist/$n.exe"
