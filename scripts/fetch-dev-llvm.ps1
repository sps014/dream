# Windows counterpart of scripts/fetch-dev-llvm.sh: the full LLVM a development build needs
# (releases ship a minimal one instead). The official LLVM release supplies clang, which builds
# the C runtime a release ships prebuilt; the wasi-sdk archive supplies compiler-rt's wasm32
# builtins (into clang's resource directory) and the WASI libc headers (share\wasi-sysroot).
#
# Installs to $env:DREAM_TOOLCHAINS or ~\.dream\toolchains\llvm-<version>. Idempotent.
# Usage: powershell -ExecutionPolicy Bypass -File scripts\fetch-dev-llvm.ps1

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$LlvmVersion = "22.1.8"
$LlvmMajor = $LlvmVersion.Split(".")[0]
$WasiSdkVersion = "33.0"
$WasiSdkArchive = "wasi-sdk-$WasiSdkVersion-x86_64-linux.tar.gz"
$WasiSdkSha = "0ba8b5bfaeb2adf3f29bab5841d76cf5318ab8e1642ea195f88baba1abd47bce"

if ($env:PROCESSOR_ARCHITECTURE -ne "AMD64") {
    throw "fetch-dev-llvm.ps1 supports x86_64 Windows; install LLVM $LlvmVersion yourself and set DREAM_LLVM"
}
$LlvmArchive = "clang+llvm-$LlvmVersion-x86_64-pc-windows-msvc.tar.xz"
$LlvmSha = "d96c2cc1736f4eb7fa43cb9bbdf56d93551a9ae0a9aadb9c99c3c3b2b712a234"

$Toolchains = if ($env:DREAM_TOOLCHAINS) { $env:DREAM_TOOLCHAINS } else { Join-Path $HOME ".dream\toolchains" }
$Dest = Join-Path $Toolchains "llvm-$LlvmVersion"
$Tools = @("clang", "opt", "llc", "llvm-link", "llvm-dis", "llvm-as", "llvm-ar", "llvm-profdata", "lld", "wasm-ld")

New-Item -ItemType Directory -Force -Path $Toolchains, $Dest | Out-Null

function Get-Verified([string]$Url, [string]$File, [string]$Sha) {
    $out = Join-Path $Toolchains $File
    $ok = (Test-Path $out) -and ((Get-FileHash $out -Algorithm SHA256).Hash -eq $Sha.ToUpper())
    if (-not $ok) {
        Write-Host "downloading $Url"
        Invoke-WebRequest -Uri $Url -OutFile "$out.part" -UseBasicParsing
        Move-Item -Force "$out.part" $out
    }
    if ((Get-FileHash $out -Algorithm SHA256).Hash -ne $Sha.ToUpper()) {
        throw "checksum mismatch for $out"
    }
    return $out
}

# bsdtar (System32\tar.exe) takes Windows paths as-is; Git's GNU tar needs --force-local.
function Expand-Tar([string]$Archive, [string]$Into, [string[]]$Patterns, [string[]]$Extra = @()) {
    $tar = Join-Path $env:SystemRoot "System32\tar.exe"
    & $tar -xf $Archive -C $Into --strip-components 1 @Extra @Patterns
    if ($LASTEXITCODE -eq 0) { return }
    $gitTar = Join-Path $env:ProgramFiles "Git\usr\bin\tar.exe"
    if (-not (Test-Path $gitTar)) {
        throw "tar could not extract $Archive (install Git for Windows for a tar with xz support)"
    }
    & $gitTar --force-local --wildcards -xf $Archive -C $Into --strip-components 1 @Extra @Patterns
    if ($LASTEXITCODE -ne 0) { throw "tar could not extract $Archive" }
}

if (-not (Test-Path (Join-Path $Dest "bin\clang.exe")) -or -not (Test-Path (Join-Path $Dest "bin\opt.exe"))) {
    $archive = Get-Verified "https://github.com/llvm/llvm-project/releases/download/llvmorg-$LlvmVersion/$LlvmArchive" $LlvmArchive $LlvmSha
    $patterns = @("*/LICENSE.TXT", "*/lib/clang/*") + ($Tools | ForEach-Object { "*/bin/$_.exe" })
    Expand-Tar $archive $Dest $patterns
    Remove-Item -Force $archive
}

$resource = Join-Path $Dest "lib\clang\$LlvmMajor\lib"
$targets = @("wasm32-unknown-wasip1", "wasm32-unknown-wasip1-threads")
$sysroot = Join-Path $Dest "share\wasi-sysroot"
$missing = ($targets | Where-Object { -not (Test-Path (Join-Path $resource "$_\libclang_rt.builtins.a")) }) -or
    -not (Test-Path (Join-Path $sysroot "include\wasm32-wasip1\string.h"))
if ($missing) {
    $archive = Get-Verified "https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-$($WasiSdkVersion.Split('.')[0])/$WasiSdkArchive" $WasiSdkArchive $WasiSdkSha
    $tmp = Join-Path ([IO.Path]::GetTempPath()) ("dream-wasi-" + [Guid]::NewGuid())
    New-Item -ItemType Directory -Force -Path $tmp | Out-Null
    try {
        $patterns = ($targets | ForEach-Object { "*/lib/clang/$LlvmMajor/lib/$_/libclang_rt.builtins.a" }) +
            @("*/share/wasi-sysroot/include/wasm32-wasip1/*")
        Expand-Tar $archive $tmp $patterns @("--exclude", "*/c++/*")
        foreach ($t in $targets) {
            New-Item -ItemType Directory -Force -Path (Join-Path $resource $t) | Out-Null
            Copy-Item -Force (Join-Path $tmp "lib\clang\$LlvmMajor\lib\$t\libclang_rt.builtins.a") (Join-Path $resource $t)
        }
        if (Test-Path $sysroot) { Remove-Item -Recurse -Force $sysroot }
        New-Item -ItemType Directory -Force -Path (Join-Path $Dest "share") | Out-Null
        Move-Item (Join-Path $tmp "share\wasi-sysroot") $sysroot
    } finally {
        Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
        Remove-Item -Force $archive -ErrorAction SilentlyContinue
    }
}

$version = & (Join-Path $Dest "bin\opt.exe") --version | Select-String "LLVM version $LlvmVersion"
if (-not $version) { throw "LLVM at $Dest is not version $LlvmVersion" }
Write-Host "LLVM $LlvmVersion ready at $Dest"
