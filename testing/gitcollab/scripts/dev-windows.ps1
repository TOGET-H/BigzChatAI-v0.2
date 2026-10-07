param([string]$Proxy, [string]$ToolchainRoot, [string]$TargetDir)
$ErrorActionPreference = 'Stop'
$gitCollabTaskRoot = [IO.Path]::GetFullPath((Split-Path $PSScriptRoot -Parent))

. (Join-Path $gitCollabTaskRoot 'scripts/windows-env.ps1') -Proxy $Proxy -ToolchainRoot $ToolchainRoot
$env:CARGO_TARGET_DIR = if ($TargetDir) { [IO.Path]::GetFullPath($TargetDir) } else { Join-Path $gitCollabTaskRoot 'target' }
$git = Get-Command git.exe -ErrorAction Stop
$gitVersion = & $git.Source --version
if ($LASTEXITCODE -or $gitVersion -notmatch 'git version (\d+\.\d+\.\d+)' -or [version]$Matches[1] -lt [version]'2.51.0') {
    throw 'Install system Git 2.51 or newer, then reopen PowerShell. Git Collab does not download Git.'
}
$webViewRoots = @("${env:ProgramFiles(x86)}\Microsoft\EdgeWebView\Application", "$env:ProgramFiles\Microsoft\EdgeWebView\Application", "$env:LOCALAPPDATA\Microsoft\EdgeWebView\Application")
$hasWebView = @($webViewRoots | Where-Object { Test-Path -LiteralPath $_ }).Count -gt 0
if (-not $hasWebView) { throw 'Install Microsoft Edge WebView2 Runtime before starting Git Collab.' }
Push-Location $gitCollabTaskRoot
try {
    & corepack.cmd pnpm@8.14.0 --fail-if-no-match --filter @git-collab/desktop tauri dev
    if ($LASTEXITCODE) { throw 'Git Collab development process failed.' }
} finally { Pop-Location }
