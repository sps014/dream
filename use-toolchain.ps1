# Windows counterpart of use-toolchain.sh: build the Dream toolchain binaries and install them for
# PowerShell + IDE use.
#
#   .\use-toolchain.ps1               # release (default)
#   .\use-toolchain.ps1 -DebugBuild   # target\debug instead
#   .\use-toolchain.ps1 -SkipBuild    # only re-link / re-export
#   .\use-toolchain.ps1 -Unlink       # remove links, env files, and user environment entries
#
# After install:
#   - `dream`, `dreamer`, `dream-lsp` work from any directory (~\.dream\bin on the user PATH)
#   - Cursor/VS Code pick up paths from ~\.dream\toolchain.env (reload the window)
# If script execution is disabled: powershell -ExecutionPolicy Bypass -File .\use-toolchain.ps1

param(
    [switch]$DebugBuild,
    [switch]$SkipBuild,
    [switch]$Unlink
)

$ErrorActionPreference = "Stop"

$Root = $PSScriptRoot
$UserDir = Join-Path $HOME ".dream"
$BinDir = Join-Path $UserDir "bin"
$EnvFile = Join-Path $UserDir "toolchain.env"
$Names = @("dream.exe", "dreamer.exe", "dream-lsp.exe")
$Capabilities = @("core", "unicode", "crypto", "process", "timezone")
$Libs = @($Capabilities | ForEach-Object {
    "dream_host_$_.dll"
    "dream_host_$_.dll.lib"
    "libdream_host_$_.dll.a"
})

function Set-UserEnv([string]$Name, [string]$Value) {
    [Environment]::SetEnvironmentVariable($Name, $Value, "User")
    Set-Item -Path "Env:$Name" -Value $Value
}

function Remove-UserEnv([string]$Name) {
    [Environment]::SetEnvironmentVariable($Name, $null, "User")
    Remove-Item -Path "Env:$Name" -ErrorAction SilentlyContinue
}

function Split-PathList([string]$List) {
    if ([string]::IsNullOrEmpty($List)) { return @() }
    return @($List -split ";" | Where-Object { $_ -and $_ -ne $BinDir })
}

if ($Unlink) {
    foreach ($n in $Names + $Libs) {
        $p = Join-Path $BinDir $n
        if (Test-Path $p) {
            Remove-Item -Force $p
            Write-Host "Removed $p"
        }
    }
    if ((Test-Path $BinDir) -and -not (Get-ChildItem $BinDir)) { Remove-Item $BinDir }
    if (Test-Path $EnvFile) {
        Remove-Item -Force $EnvFile
        Write-Host "Removed $EnvFile"
    }
    foreach ($v in @("DREAM_HOME", "DREAMER_HOME", "DREAM_BIN")) { Remove-UserEnv $v }
    $userPath = Split-PathList ([Environment]::GetEnvironmentVariable("Path", "User"))
    [Environment]::SetEnvironmentVariable("Path", ($userPath -join ";"), "User")
    $env:Path = (Split-PathList $env:Path) -join ";"
    Write-Host "Dream toolchain unlinked. Open a new terminal (or reload the IDE) so nothing still sees the old paths."
    return
}

$BuildProfile = if ($DebugBuild) { "debug" } else { "release" }
$DreamHome = Join-Path $Root "target\$BuildProfile"

if (-not $SkipBuild) {
    Write-Host "Building $BuildProfile toolchain and core host capabilities..."
    $packages = @("dream", "dream-host", "dream-lsp", "dreamer") + @($Capabilities | ForEach-Object { "dream-host-$_" })
    $cargoArgs = @("build")
    foreach ($package in $packages) { $cargoArgs += @("-p", $package) }
    if ($BuildProfile -eq "release") { $cargoArgs += "--release" }
    Push-Location $Root
    try {
        & cargo @cargoArgs
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
    } finally {
        Pop-Location
    }
}

foreach ($n in $Names + @($Capabilities | ForEach-Object { "dream_host_$_.dll" })) {
    if (-not (Test-Path (Join-Path $DreamHome $n))) {
        throw "missing $(Join-Path $DreamHome $n); build failed or omit -SkipBuild"
    }
}

New-Item -ItemType Directory -Force -Path $BinDir | Out-Null

# Symbolic links need Developer Mode or an elevated shell; without them fall back to copies,
# which (unlike links) need this script re-run after each rebuild.
$copied = $false
foreach ($n in $Names + $Libs) {
    $src = Join-Path $DreamHome $n
    if (-not (Test-Path $src)) { continue }
    $dst = Join-Path $BinDir $n
    if (Test-Path $dst) { Remove-Item -Force $dst }
    try {
        New-Item -ItemType SymbolicLink -Path $dst -Target $src | Out-Null
    } catch {
        Copy-Item -Force $src $dst
        $copied = $true
    }
}
if ($copied) {
    Write-Host "Copied binaries into $BinDir (enable Developer Mode for links that follow rebuilds)"
} else {
    Write-Host "Linked $BinDir\{dream,dreamer,dream-lsp}.exe -> $DreamHome\"
}

$rtSrc = Join-Path $Root "crates\dream-mir\src\runtime\c"
if (Test-Path (Join-Path $rtSrc "core\include\dream_core.h")) {
    $rtDst = Join-Path $UserDir "lib\runtime\c"
    New-Item -ItemType Directory -Force -Path (Split-Path $rtDst) | Out-Null
    if (Test-Path $rtDst) { Remove-Item -Recurse -Force $rtDst }
    Copy-Item -Recurse $rtSrc $rtDst
    Write-Host "Copied runtime C -> $rtDst"
}

Set-UserEnv "DREAM_HOME" $DreamHome
Set-UserEnv "DREAMER_HOME" $DreamHome
Set-UserEnv "DREAM_BIN" (Join-Path $DreamHome "dream.exe")

$userPath = Split-PathList ([Environment]::GetEnvironmentVariable("Path", "User"))
[Environment]::SetEnvironmentVariable("Path", (@($BinDir) + $userPath) -join ";", "User")
$env:Path = (@($BinDir) + (Split-PathList $env:Path)) -join ";"

@"
# Written by use-toolchain.ps1 - read by the VS Code/Cursor Dream extension and dreamer.
DREAM_HOME=$env:DREAM_HOME
DREAMER_HOME=$env:DREAMER_HOME
DREAM_BIN=$env:DREAM_BIN
"@ | Set-Content -Path $EnvFile -Encoding utf8
Write-Host "Wrote $EnvFile"

# `dreamer toolchain install` records DREAM_ZIG here; env.sh sources it on Unix.
$toolchainsEnv = Join-Path $UserDir "toolchains.env"
function Import-ToolchainsEnv {
    if (-not (Test-Path $toolchainsEnv)) { return }
    foreach ($line in Get-Content $toolchainsEnv) {
        if ($line -match '^\s*([A-Za-z_][A-Za-z0-9_]*)=(.*)$') { Set-UserEnv $Matches[1] $Matches[2] }
    }
}
Import-ToolchainsEnv

Write-Host "DREAM_HOME=$env:DREAM_HOME"
Write-Host "Ready: dream=$((Get-Command dream).Source)  dreamer=$((Get-Command dreamer).Source)  dream-lsp=$((Get-Command dream-lsp).Source)"
Write-Host "New terminals pick this up automatically; this one is already configured."
Write-Host "Reload the Cursor/VS Code window if the LSP was already running."
Write-Host "To remove:  .\use-toolchain.ps1 -Unlink"

function Test-Compiler([string]$Value) {
    if ([string]::IsNullOrEmpty($Value)) { return $false }
    if (Test-Path -LiteralPath $Value -PathType Leaf) { return $true }
    return $null -ne (Get-Command $Value -ErrorAction SilentlyContinue)
}

if ($env:DREAM_SKIP_CC -eq "1") {
    Write-Host "Skipped C compiler install (DREAM_SKIP_CC=1)"
} else {
    $hasCc = (Test-Compiler $env:DREAM_CC) -or (Test-Compiler $env:CC) -or (Test-Compiler $env:DREAM_ZIG) -or
        (@(Get-ChildItem -Path (Join-Path $UserDir "toolchains\zig-*\zig.exe") -ErrorAction SilentlyContinue).Count -gt 0) -or
        (@("cc", "clang", "zig") | Where-Object { Get-Command $_ -ErrorAction SilentlyContinue })
    if ($hasCc) {
        Write-Host "C compiler already found; skipped dreamer toolchain install cc"
    } else {
        Write-Host "No C compiler on PATH; installing via dreamer toolchain install cc"
        & dreamer toolchain install cc
        if ($LASTEXITCODE -eq 0) {
            Import-ToolchainsEnv
        } else {
            Write-Warning "could not install a C compiler; later run: dreamer toolchain install cc"
        }
    }
}

# A development build compiles the C runtime itself, so it needs the full official LLVM (plus the
# wasm32 builtins and WASI headers); releases ship a minimal LLVM and the runtime prebuilt.
if ($env:DREAM_SKIP_LLVM -eq "1") {
    Write-Host "Skipped LLVM fetch (DREAM_SKIP_LLVM=1)"
} elseif ($env:DREAM_LLVM) {
    Write-Host "DREAM_LLVM is set; skipped scripts\fetch-dev-llvm.ps1"
} else {
    try {
        & (Join-Path $Root "scripts\fetch-dev-llvm.ps1")
    } catch {
        Write-Warning "could not fetch LLVM ($_); later run: scripts\fetch-dev-llvm.ps1"
    }
}

Write-Host "Optimized WASM builds install pinned Binaryen through Dreamer on first use. For offline setup: dreamer toolchain install binaryen"
