# Install mgc binary from GitHub release on Windows PowerShell
# Usage: irm https://raw.githubusercontent.com/mingd-153/MagiCore/main/scripts/install.ps1 | iex

[CmdletBinding()]
param (
    [string]$Package = "magicore",
    [string]$Version = "latest",
    [string]$InstallDir = "$env:LOCALAPPDATA\Programs\MagiCore"
)

$ErrorActionPreference = "Stop"
$Repo = "mingd-153/MagiCore"

Write-Host "╔══════════════════════════════════════════════════════════════╗" -ForegroundColor Cyan
Write-Host "║     MagiCore Installer for Windows                          ║" -ForegroundColor Cyan
Write-Host "╚══════════════════════════════════════════════════════════════╝" -ForegroundColor Cyan
Write-Host ""

if ($Package -ne "magicore" -and $Package -ne "magicore-web") {
    Write-Error "Unsupported package: $Package. Expected 'magicore' or 'magicore-web'."
    exit 1
}

# Detect Architecture
$Arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString().ToLower()
$ArchLabel = switch ($Arch) {
    "x64"   { "X64" }
    "arm64" { "ARM64" }
    default { 
        Write-Error "Unsupported architecture: $Arch"
        exit 1
    }
}

Write-Host "Detected Platform: Windows ($ArchLabel)" -ForegroundColor Gray

# Resolve Target Release URL
if ($Version -eq "latest") {
    $ReleaseApiUrl = "https://api.github.com/repos/$Repo/releases/latest"
    try {
        $ReleaseData = Invoke-RestMethod -Uri $ReleaseApiUrl -UseBasicParsing
        $Tag = $ReleaseData.tag_name
    } catch {
        $Tag = "v0.2.0"
    }
} else {
    $Tag = $Version
}

$ArchiveName = "$Package-Windows-$ArchLabel.zip"
$DownloadUrl = "https://github.com/$Repo/releases/download/$Tag/$ArchiveName"
$ChecksumUrl = "$DownloadUrl.sha256"

$TempDir = [System.IO.Path]::GetTempPath()
$ZipPath = Join-Path $TempDir $ArchiveName
$ChecksumPath = Join-Path $TempDir "$ArchiveName.sha256"

Write-Host "Downloading MagiCore ($Tag)..." -ForegroundColor Yellow
try {
    Invoke-WebRequest -Uri $DownloadUrl -OutFile $ZipPath -UseBasicParsing
    Invoke-WebRequest -Uri $ChecksumUrl -OutFile $ChecksumPath -UseBasicParsing
} catch {
    Write-Warning "Could not download GitHub release archive ($DownloadUrl)."
    Write-Error "Prebuilt release artifact is required. Refusing to fall back to another installer."
    exit 1
}

# Verify SHA-256 Checksum
Write-Host "Verifying SHA-256 integrity..." -ForegroundColor Gray
$ExpectedHash = (Get-Content $ChecksumPath).Trim().Split()[0].ToLower()
$ActualHash = (Get-FileHash -Path $ZipPath -Algorithm SHA256).Hash.ToLower()

if ($ExpectedHash -ne $ActualHash) {
    Write-Error "Checksum verification failed! Expected: $ExpectedHash, Got: $ActualHash"
    exit 1
}

# Extract and Install
Write-Host "Extracting to $InstallDir..." -ForegroundColor Gray
if (!(Test-Path $InstallDir)) {
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
}

Expand-Archive -Path $ZipPath -DestinationPath $InstallDir -Force
$ExePath = Join-Path $InstallDir "mgc.exe"

# Add to User PATH if not present
$UserPath = [Environment]::GetEnvironmentVariable("Path", [EnvironmentVariableTarget]::User)
if ($UserPath -notlike "*$InstallDir*") {
    Write-Host "Adding $InstallDir to User PATH..." -ForegroundColor Yellow
    [Environment]::SetEnvironmentVariable("Path", "$UserPath;$InstallDir", [EnvironmentVariableTarget]::User)
    $env:Path = "$env:Path;$InstallDir"
}

Write-Host ""
Write-Host "✅ MagiCore installed successfully to: $ExePath" -ForegroundColor Green
Write-Host "Run 'mgc --help' to get started." -ForegroundColor Cyan
