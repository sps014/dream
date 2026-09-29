# Builds and packages the Dream VS Code extension (.vsix).
# The extension does not bundle dream / dream-lsp / dreamer - point it at a local toolchain
# with `. .\use-toolchain.ps1` or settings dream.home / dreamer.home.

$ErrorActionPreference = 'Stop'

$VscodeDir = Join-Path $PSScriptRoot 'vscode'

function Invoke-Native {
    param([string]$Command, [string[]]$Arguments)
    & $Command @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "'$Command $($Arguments -join ' ')' failed with exit code $LASTEXITCODE"
    }
}

Write-Host '==> Navigating to VS Code extension directory...'
Push-Location $VscodeDir
try {
    Write-Host '==> Installing dependencies...'
    Invoke-Native npm.cmd @('install')

    Write-Host '==> Compiling TypeScript...'
    Invoke-Native npm.cmd @('run', 'compile')

    Write-Host '==> Packaging extension into .vsix...'
    Invoke-Native npx.cmd @('@vscode/vsce', 'package')

    $vsix = Get-ChildItem -Filter *.vsix | Sort-Object LastWriteTime -Descending | Select-Object -First 1
    Write-Host '==> Done! You can install the extension with:'
    Write-Host "    code --install-extension tooling/vscode/$($vsix.Name)"
    Write-Host ''
    Write-Host 'Before using it, make the toolchain available:'
    Write-Host '    . .\use-toolchain.ps1'
    Write-Host '    # or set VS Code settings dream.home / dreamer.home'
}
finally {
    Pop-Location
}
