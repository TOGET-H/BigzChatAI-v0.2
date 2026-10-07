param([string]$Proxy, [string]$ToolchainRoot)
$ErrorActionPreference = 'Stop'
$gitCollabEnvRoot = [IO.Path]::GetFullPath((Split-Path $PSScriptRoot -Parent))
if (-not [Environment]::Is64BitProcess) { throw 'Use 64-bit PowerShell on Windows x64.' }
$gitCollabToolsRoot = if ($ToolchainRoot) { [IO.Path]::GetFullPath($ToolchainRoot) } else { $gitCollabEnvRoot }
$gitCollabLocalCargo = Join-Path $gitCollabToolsRoot '.tools/cargo'
if (Test-Path -LiteralPath (Join-Path $gitCollabLocalCargo 'bin/cargo.exe')) {
    $env:CARGO_HOME = $gitCollabLocalCargo
    $env:RUSTUP_HOME = Join-Path $gitCollabToolsRoot '.tools/rustup'
    $env:RUSTUP_TOOLCHAIN = '1.95.0'
    $env:PATH = "$gitCollabLocalCargo\bin;$env:PATH"
}
$gitCollabCorepack = Join-Path $gitCollabToolsRoot '.tools/corepack'
if ($ToolchainRoot -and (Test-Path -LiteralPath $gitCollabCorepack)) {
    $env:COREPACK_HOME = $gitCollabCorepack
}
if ($Proxy) {
    $env:HTTPS_PROXY = $Proxy
    $env:HTTP_PROXY = $Proxy
    $env:CARGO_HTTP_PROXY = $Proxy
}
# Discover installed MSVC/SDK. No download or global configuration writes.
$gitCollabVsRoots = @("${env:ProgramFiles(x86)}\Microsoft Visual Studio", "$env:ProgramFiles\Microsoft Visual Studio")
$gitCollabToolsets = foreach ($gitCollabVsRoot in $gitCollabVsRoots) {
    Get-ChildItem "$gitCollabVsRoot\*\*\VC\Tools\MSVC\*" -Directory -ErrorAction SilentlyContinue
}
$gitCollabToolset = $gitCollabToolsets | Sort-Object Name -Descending | Select-Object -First 1
$gitCollabKitRoot = "${env:ProgramFiles(x86)}\Windows Kits\10"
$gitCollabKit = Get-ChildItem "$gitCollabKitRoot\Lib" -Directory -ErrorAction SilentlyContinue | Sort-Object Name -Descending | Select-Object -First 1
if ($gitCollabToolset -and $gitCollabKit) {
    $env:PATH = "$($gitCollabToolset.FullName)\bin\Hostx64\x64;$gitCollabKitRoot\bin\$($gitCollabKit.Name)\x64;$env:PATH"
    $env:LIB = "$($gitCollabToolset.FullName)\lib\x64;$($gitCollabKit.FullName)\um\x64;$($gitCollabKit.FullName)\ucrt\x64"
    $env:INCLUDE = "$($gitCollabToolset.FullName)\include;$gitCollabKitRoot\Include\$($gitCollabKit.Name)\ucrt;$gitCollabKitRoot\Include\$($gitCollabKit.Name)\shared;$gitCollabKitRoot\Include\$($gitCollabKit.Name)\um"
}
