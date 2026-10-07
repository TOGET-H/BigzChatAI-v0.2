param([string]$Proxy, [string]$ToolchainRoot, [string]$TargetDir, [switch]$PortableOnly)
$ErrorActionPreference = 'Stop'
Import-Module Microsoft.PowerShell.Utility -ErrorAction Stop
$gitCollabTaskRoot = [IO.Path]::GetFullPath((Split-Path $PSScriptRoot -Parent))

$gitCollabCorepackHome = $env:COREPACK_HOME
. (Join-Path $gitCollabTaskRoot 'scripts/windows-env.ps1') -Proxy $Proxy -ToolchainRoot $ToolchainRoot
if ($gitCollabCorepackHome) { $env:COREPACK_HOME = $gitCollabCorepackHome }
$env:CARGO_TARGET_DIR = if ($TargetDir) { [IO.Path]::GetFullPath($TargetDir) } else { Join-Path $gitCollabTaskRoot 'target' }
$buildTarget = if ($env:CARGO_TARGET_DIR) { [IO.Path]::GetFullPath($env:CARGO_TARGET_DIR) } else { Join-Path $gitCollabTaskRoot 'target' }
$git = Get-Command git.exe -ErrorAction Stop
$gitVersion = & $git.Source --version
if ($LASTEXITCODE -or $gitVersion -notmatch 'git version (\d+\.\d+\.\d+)' -or [version]$Matches[1] -lt [version]'2.51.0') { throw 'Git Collab requires system Git 2.51 or newer. No Git download is performed.' }
$webViewRoots = @("${env:ProgramFiles(x86)}\Microsoft\EdgeWebView\Application", "$env:ProgramFiles\Microsoft\EdgeWebView\Application", "$env:LOCALAPPDATA\Microsoft\EdgeWebView\Application")
if (@($webViewRoots | Where-Object { Test-Path -LiteralPath $_ }).Count -eq 0) { throw 'Git Collab requires Microsoft Edge WebView2 Runtime.' }
$nsisCache = Join-Path $env:LOCALAPPDATA 'tauri/NSIS'
$bundleNsis = (-not $PortableOnly) -and (Test-Path -LiteralPath (Join-Path $nsisCache 'makensis.exe'))
if (-not $PortableOnly -and -not $bundleNsis) { Write-Warning 'Cached NSIS unavailable; building portable EXE only. No installer was created.' }
if (-not (Test-Path -LiteralPath (Join-Path $gitCollabTaskRoot 'docs/验收与使用说明.md') -PathType Leaf)) { throw 'docs/验收与使用说明.md is required before packaging.' }
Push-Location $gitCollabTaskRoot
try {
    $sourceHead = (& git rev-parse HEAD).Trim()
    if ($LASTEXITCODE) { throw 'Source HEAD is unavailable.' }
    if (& git status --porcelain --untracked-files=no) { throw 'Commit tracked source changes before packaging so artifacts correspond to an exact source HEAD.' }
    $sourceFiles = @(& git -c core.quotepath=false ls-files -- apps/desktop crates/collaboration-core scripts Cargo.toml Cargo.lock package.json pnpm-lock.yaml pnpm-workspace.yaml docs/验收与使用说明.md)
    if ($LASTEXITCODE) { throw 'Source input inventory failed.' }
    $manifestPath = Join-Path $gitCollabTaskRoot 'apps/desktop/src-tauri/Cargo.toml'
    $manifestBefore = [IO.File]::ReadAllBytes($manifestPath)
    $sourceInputs = @($sourceFiles | ForEach-Object { [ordered]@{ path = $_; sha256 = (Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash.ToLower() } })
    # Use the Tauri build pipeline: custom-protocol embeds the production frontend.
    $buildStarted = Get-Date
    $priorRustFlags = $env:RUSTFLAGS
    $env:RUSTFLAGS = "$priorRustFlags -C target-feature=+crt-static".Trim()
    try {
        if ($bundleNsis) { & corepack.cmd pnpm@8.14.0 --fail-if-no-match --filter @git-collab/desktop tauri build --bundles nsis -- --locked }
        else { & corepack.cmd pnpm@8.14.0 --fail-if-no-match --filter @git-collab/desktop tauri build --no-bundle -- --locked }
        if ($LASTEXITCODE) { throw 'Git Collab Tauri release build failed. No distribution was staged.' }
    } finally { $env:RUSTFLAGS = $priorRustFlags }
    # Tauri may serialize this TOML with LF. Restore only equivalent line endings;
    # any other character change remains an error and no distribution is staged.
    $manifestAfter = [IO.File]::ReadAllBytes($manifestPath)
    if ([Convert]::ToBase64String($manifestBefore) -ne [Convert]::ToBase64String($manifestAfter)) {
        $manifestTextBefore = [Text.Encoding]::UTF8.GetString($manifestBefore).Replace("`r`n", "`n")
        $manifestTextAfter = [Text.Encoding]::UTF8.GetString($manifestAfter).Replace("`r`n", "`n")
        if ($manifestTextBefore -cne $manifestTextAfter) { throw 'Tauri changed Cargo.toml content beyond line endings; rebuild from reviewed source.' }
        [IO.File]::WriteAllBytes($manifestPath, $manifestBefore)
        Write-Host 'Restored byte-identical source Cargo.toml after Tauri line-ending normalization.'
    }
    if ((& git rev-parse HEAD).Trim() -ne $sourceHead -or (& git status --porcelain --untracked-files=no)) { throw 'Source changed during build; rebuild from a committed checkout.' }
    foreach ($inputFile in $sourceInputs) {
        if ((Get-FileHash -LiteralPath $inputFile.path -Algorithm SHA256).Hash.ToLower() -ne $inputFile.sha256) { throw "Build input changed: $($inputFile.path). Rebuild required." }
    }
    $frontendAssets = @(Get-ChildItem -LiteralPath 'apps/desktop/dist' -File -Recurse | ForEach-Object { [ordered]@{ path = $_.FullName.Substring($gitCollabTaskRoot.Length + 1).Replace('\', '/'); sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLower() } })
    $binary = Join-Path $buildTarget 'release/git-collab-desktop.exe'
    if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) { throw 'Git Collab release EXE is missing.' }
    $version = (Get-Content -LiteralPath 'apps/desktop/package.json' -Raw | ConvertFrom-Json).version
    $output = [IO.Path]::GetFullPath((Join-Path $gitCollabTaskRoot 'releases'))
    $name = "Git-Collab-$version-windows-x64-$(Get-Date -Format yyyyMMdd-HHmmss)-$([guid]::NewGuid().ToString('N').Substring(0,8))"
    $stage = [IO.Path]::GetFullPath((Join-Path $output "$name-portable"))
    if (-not $stage.StartsWith($output + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Distribution stage escaped releases directory.' }
    if (Test-Path -LiteralPath $stage) { throw 'Distribution stage already exists; no existing release will be overwritten.' }
    New-Item -ItemType Directory -Path $stage | Out-Null
    Copy-Item -LiteralPath $binary -Destination (Join-Path $stage 'Git-Collab.exe')
    Copy-Item -LiteralPath 'docs/验收与使用说明.md' -Destination (Join-Path $stage '使用说明.md')
    $files = @('Git-Collab.exe','使用说明.md')
    $hashLines = foreach ($file in $files) { "$((Get-FileHash -LiteralPath (Join-Path $stage $file) -Algorithm SHA256).Hash.ToLower())  $file" }
    $hashLines | Set-Content -LiteralPath (Join-Path $stage 'SHA256SUMS.txt') -Encoding utf8
    $files += 'SHA256SUMS.txt'
    $zipPath = Join-Path $output "$name-portable.zip"
    # Exact allowlist: executable, usage, checksums. Never recurse through the workspace.
    Compress-Archive -LiteralPath @($files | ForEach-Object { Join-Path $stage $_ }) -DestinationPath $zipPath -CompressionLevel Optimal
    $artifacts = @($zipPath)
    if ($bundleNsis) {
        $installer = Get-ChildItem -LiteralPath (Join-Path $buildTarget 'release/bundle/nsis') -File -Filter '*setup.exe' | Where-Object { ($_.Name -like '*Git 协作台*' -or $_.Name -like '*git-collab*') -and $_.LastWriteTime -ge $buildStarted.AddSeconds(-2) } | Sort-Object LastWriteTime -Descending | Select-Object -First 1
        if (-not $installer) { throw 'NSIS build returned success but no Git Collab installer was found.' }
        $installerPath = Join-Path $output "$name-setup.exe"
        Copy-Item -LiteralPath $installer.FullName -Destination $installerPath
        $artifacts += $installerPath
    }
    $artifacts | ForEach-Object { "$((Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash.ToLower())  $([IO.Path]::GetFileName($_))" } | Set-Content -LiteralPath (Join-Path $output "$name-SHA256SUMS.txt") -Encoding ascii
    $buildEvidence = [ordered]@{ sourceHead = $sourceHead; sourceInputs = $sourceInputs; frontendAssets = $frontendAssets; customProtocol = $true; rustFlags = "$priorRustFlags -C target-feature=+crt-static".Trim(); portableAllowlist = $files; binarySha256 = (Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash.ToLower(); artifacts = @($artifacts | ForEach-Object { [ordered]@{ path = $_; sha256 = (Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash.ToLower() } }) }
    $buildEvidence | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $output "$name-build-evidence.json") -Encoding utf8
    Write-Host "Source HEAD: $sourceHead"
    Write-Host "Build evidence: $(Join-Path $output "$name-build-evidence.json")"
    Write-Host "Portable EXE: $(Join-Path $stage 'Git-Collab.exe')"
    Write-Host "Portable ZIP: $zipPath"
    if ($bundleNsis) { Write-Host "NSIS installer: $installerPath (built only; not installed)" }
} finally { Pop-Location }
