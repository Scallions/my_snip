$ErrorActionPreference = 'Stop'
Push-Location (Join-Path $PSScriptRoot '..')
try {
    & npm.cmd run verify
    if ($LASTEXITCODE -ne 0) { throw 'Frontend verification failed.' }
    & cargo test --locked --manifest-path src-tauri/Cargo.toml --lib
    if ($LASTEXITCODE -ne 0) { throw 'Rust verification failed.' }
    & npm.cmd run desktop:build -- --bundles nsis
    if ($LASTEXITCODE -ne 0) { throw 'Desktop build failed.' }
    New-Item -ItemType Directory -Force -Path output | Out-Null
    $snipVersion = (Get-Content -LiteralPath package.json -Raw | ConvertFrom-Json).version
    Copy-Item -LiteralPath 'src-tauri/target/release/my-snip.exe' -Destination "output/My-Snip-$snipVersion.exe" -Force
    Get-ChildItem -LiteralPath 'src-tauri/target/release/bundle/nsis' -Filter '*.exe' | Copy-Item -Destination output -Force
    Copy-Item -LiteralPath 'THIRD_PARTY_NOTICES.md' -Destination output -Force
    Copy-Item -LiteralPath licenses -Destination output -Recurse -Force
    Get-ChildItem -LiteralPath output -Filter '*.exe' | Get-FileHash -Algorithm SHA256 | Format-Table
} finally { Pop-Location }
