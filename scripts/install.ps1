# Install Dream toolchain (dream, dreamer, dream-lsp) from GitHub Releases.
#
#   irm https://sps014.github.io/dream/install.ps1 | iex
#
# Env:
#   $env:DREAM_VERSION  optional tag without leading v (default: latest)
#   $env:DREAM_HOME     install prefix (default: $HOME\.dream)
#   $env:DREAM_SKIP_CC=1 skip auto `dreamer toolchain install cc` when no compiler is found

$ErrorActionPreference = "Stop"
$Repo = if ($env:DREAM_REPO) { $env:DREAM_REPO } else { "sps014/dream" }
$Prefix = if ($env:DREAM_HOME) { $env:DREAM_HOME } else { Join-Path $HOME ".dream" }
$BinDir = Join-Path $Prefix "bin"

function Get-Target {
    $arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
    $osArch = switch ($arch) {
        "X64" { "x64" }
        "Arm64" { "arm64" }
        default { throw "unsupported arch: $arch" }
    }
    if ($IsWindows -or $env:OS -match "Windows") {
        return "windows-$osArch"
    }
    if ($IsMacOS) { return "macos-$osArch" }
    if ($IsLinux) { return "linux-$osArch" }
    # Windows PowerShell 5.x
    return "windows-$osArch"
}

$Target = Get-Target
if ($env:DREAM_VERSION) {
    $Tag = "v" + ($env:DREAM_VERSION -replace '^v', '')
} else {
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest"
    $Tag = $release.tag_name
}

if (-not $Tag) {
    Write-Error @"
no Dream release found on https://github.com/$Repo/releases

Publish a tagged release, or build from source with Rust:
  git clone https://github.com/$Repo.git
  cd dream
  source ./use-toolchain.sh
"@
}

$Version = $Tag.TrimStart('v')
$Archive = "dream-$Version-$Target.zip"
$Base = "https://github.com/$Repo/releases/download/$Tag"
$Url = "$Base/$Archive"
$Work = Join-Path ([System.IO.Path]::GetTempPath()) ("dream-install-" + [guid]::NewGuid().ToString("n"))
New-Item -ItemType Directory -Path $Work | Out-Null
try {
    Write-Host "Installing Dream $Version ($Target) -> $Prefix"
    Write-Host "Downloading $Url"
    $ArchivePath = Join-Path $Work $Archive
    Invoke-WebRequest -Uri $Url -OutFile $ArchivePath

    $SumUrl = "$Base/SHA256SUMS"
    try {
        $sums = Invoke-WebRequest -Uri $SumUrl
        $line = ($sums.Content -split "`n" | Where-Object { $_ -match [regex]::Escape($Archive) } | Select-Object -First 1)
        if ($line) {
            $expected = ($line -split '\s+')[0].Trim()
            $actual = (Get-FileHash -Algorithm SHA256 -Path $ArchivePath).Hash.ToLowerInvariant()
            if ($actual -ne $expected.ToLowerInvariant()) {
                throw "checksum mismatch for $Archive (expected $expected, got $actual)"
            }
            Write-Host "Checksum OK"
        }
    } catch {
        # SHA256SUMS optional
    }

    Expand-Archive -Path $ArchivePath -DestinationPath (Join-Path $Work "out") -Force
    New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
    Get-ChildItem -Path (Join-Path $Work "out") -Recurse -File |
        Where-Object { $_.Name -match '^(dream|dreamer|dream-lsp)(\.exe)?$' } |
        ForEach-Object { Copy-Item $_.FullName -Destination $BinDir -Force }
    Get-ChildItem -Path (Join-Path $Work "out") -Recurse -Directory |
        Where-Object { $_.FullName -match '[\\/]lib[\\/]runtime[\\/]c$' } |
        Select-Object -First 1 |
        ForEach-Object {
            $libRt = Join-Path $Prefix "lib\runtime"
            New-Item -ItemType Directory -Force -Path $libRt | Out-Null
            $destC = Join-Path $libRt "c"
            if (Test-Path $destC) { Remove-Item -Recurse -Force $destC }
            Copy-Item $_.FullName -Destination $destC -Recurse -Force
        }

    $Ext = if ($Target -like "windows-*") { ".exe" } else { "" }
    @"
DREAM_HOME=$BinDir
DREAMER_HOME=$BinDir
DREAM_BIN=$BinDir\dream$Ext
"@ | Set-Content -Path (Join-Path $Prefix "toolchain.env") -Encoding utf8

    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if (-not ($userPath -split ';' | Where-Object { $_ -eq $BinDir })) {
        [Environment]::SetEnvironmentVariable("Path", ($BinDir + ";" + $userPath), "User")
        Write-Host "Added $BinDir to user PATH"
    }
    $env:Path = "$BinDir;$env:Path"
    $env:DREAM_HOME = $BinDir
    $env:DREAMER_HOME = $BinDir
    $env:DREAM_BIN = Join-Path $BinDir "dream$Ext"

    function Test-EnvCompiler([string]$Value) {
        if ([string]::IsNullOrEmpty($Value)) { return $false }
        if (Test-Path -LiteralPath $Value -PathType Leaf) { return $true }
        return $null -ne (Get-Command $Value -ErrorAction SilentlyContinue)
    }

    function Test-HasCc {
        if (Test-EnvCompiler $env:DREAM_CC) { return $true }
        if (Test-EnvCompiler $env:CC) { return $true }
        if (Test-EnvCompiler $env:DREAM_ZIG) { return $true }
        $tc = Join-Path $Prefix "toolchains"
        if (Test-Path $tc) {
            foreach ($dir in (Get-ChildItem -Path $tc -Directory -ErrorAction SilentlyContinue | Where-Object { $_.Name -like "zig-*" })) {
                if (Test-Path (Join-Path $dir.FullName "zig.exe")) { return $true }
                if (Test-Path (Join-Path $dir.FullName "zig")) { return $true }
            }
        }
        foreach ($name in @("cc", "clang", "zig")) {
            if (Get-Command $name -ErrorAction SilentlyContinue) { return $true }
        }
        return $false
    }

    $CcNote = $null
    if ($env:DREAM_SKIP_CC -eq "1") {
        $CcNote = "Skipped C compiler install (DREAM_SKIP_CC=1)"
        Write-Host $CcNote
    } elseif (Test-HasCc) {
        $CcNote = "C compiler already found; skipped dreamer toolchain install cc"
        Write-Host $CcNote
    } else {
        Write-Host "No C compiler on PATH; installing via dreamer toolchain install cc"
        $dreamer = Join-Path $BinDir "dreamer$Ext"
        $prevEap = $ErrorActionPreference
        $ErrorActionPreference = "Continue"
        try {
            & $dreamer toolchain install cc
            if ($LASTEXITCODE -eq 0) {
                $CcNote = "Installed C compiler (Zig) via dreamer toolchain install cc"
            } else {
                $CcNote = "warning: could not install a C compiler; later run: dreamer toolchain install cc"
                Write-Host $CcNote
            }
        } finally {
            $ErrorActionPreference = $prevEap
        }
    }

    Write-Host ""
    Write-Host "Installed:"
    Write-Host "  $BinDir\dream$Ext"
    Write-Host "  $BinDir\dreamer$Ext"
    Write-Host "  $BinDir\dream-lsp$Ext"
    if ($CcNote) {
        Write-Host "  $CcNote"
    }
    Write-Host ""
    Write-Host "Open a new terminal, then: dreamer init hello"
} finally {
    Remove-Item -Recurse -Force $Work -ErrorAction SilentlyContinue
}
