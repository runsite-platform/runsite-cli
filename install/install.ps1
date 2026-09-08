# RunSite CLI installer for Windows.
#
# Usage:
#   irm https://raw.githubusercontent.com/runsite-platform/runsite-cli/master/install/install.ps1 | iex
#
# Environment overrides:
#   $env:RUNSITE_VERSION     pin a specific version (e.g. v0.1.0). Default: latest
#   $env:RUNSITE_INSTALL_DIR install directory. Default: $env:LOCALAPPDATA\runsite\bin
#   $env:RUNSITE_REPO        GitHub repo. Default: runsite-platform/runsite-cli

$ErrorActionPreference = 'Stop'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$Repo        = if ($env:RUNSITE_REPO)        { $env:RUNSITE_REPO }        else { 'runsite-platform/runsite-cli' }
$Version     = if ($env:RUNSITE_VERSION)     { $env:RUNSITE_VERSION }     else { 'latest' }
$InstallDir  = if ($env:RUNSITE_INSTALL_DIR) { $env:RUNSITE_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'runsite\bin' }

if (-not [Environment]::Is64BitOperatingSystem) {
    throw 'Only 64-bit Windows is supported'
}
$target = 'x86_64-pc-windows-msvc'

if ($Version -eq 'latest') {
    $api = "https://api.github.com/repos/$Repo/releases/latest"
    $headers = @{ 'User-Agent' = 'runsite-installer' }
    $release = Invoke-RestMethod -Uri $api -Headers $headers
    $Version = $release.tag_name
    if (-not $Version) { throw 'Could not determine latest version' }
}

$archive = "runsite-$Version-$target.zip"
$base    = "https://github.com/$Repo/releases/download/$Version"
$url     = "$base/$archive"

Write-Host "Installing runsite $Version for $target"
Write-Host "  from $url"

$tmpZip = Join-Path ([System.IO.Path]::GetTempPath()) ([System.IO.Path]::GetRandomFileName() + '.zip')
$tmpDir = Join-Path ([System.IO.Path]::GetTempPath()) ([System.IO.Path]::GetRandomFileName())

try {
    Invoke-WebRequest -Uri $url -OutFile $tmpZip -UseBasicParsing

    # Verify against the release's SHA256SUMS.txt before unpacking anything.
    $sums = (Invoke-WebRequest -Uri "$base/SHA256SUMS.txt" -UseBasicParsing).Content
    $line = $sums -split "`n" | Where-Object { $_ -match "\s\*?$([regex]::Escape($archive))\s*$" } | Select-Object -First 1
    if (-not $line) { throw "No checksum listed for $archive" }
    $expected = ($line -split '\s+')[0]
    $actual = (Get-FileHash -Path $tmpZip -Algorithm SHA256).Hash.ToLower()
    if ($expected.ToLower() -ne $actual) {
        throw "Checksum mismatch for $archive`n  expected: $expected`n  actual:   $actual"
    }

    New-Item -ItemType Directory -Path $tmpDir -Force | Out-Null
    Expand-Archive -Path $tmpZip -DestinationPath $tmpDir -Force

    $binary = Join-Path $tmpDir 'runsite.exe'
    if (-not (Test-Path $binary)) { throw "runsite.exe not found in archive" }

    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    Move-Item -Path $binary -Destination (Join-Path $InstallDir 'runsite.exe') -Force
}
finally {
    if (Test-Path $tmpZip) { Remove-Item $tmpZip -Force -ErrorAction SilentlyContinue }
    if (Test-Path $tmpDir) { Remove-Item $tmpDir -Recurse -Force -ErrorAction SilentlyContinue }
}

Write-Host ""
Write-Host "Installed: $InstallDir\runsite.exe"

$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$pathEntries = if ($userPath) { $userPath -split ';' } else { @() }
if ($pathEntries -notcontains $InstallDir) {
    $newPath = if ($userPath) { "$userPath;$InstallDir" } else { $InstallDir }
    [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
    Write-Host ""
    Write-Host "Added $InstallDir to user PATH."
    Write-Host "Open a new terminal and run: runsite --help"
} else {
    Write-Host "Run: runsite --help"
}
