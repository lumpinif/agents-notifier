param(
    [string]$Repo = $(if ($env:AGENTS_NOTIFIER_REPO) { $env:AGENTS_NOTIFIER_REPO } else { "lumpinif/agents-notifier" }),
    [string]$InstallDir = $(if ($env:AGENTS_NOTIFIER_INSTALL_DIR) { $env:AGENTS_NOTIFIER_INSTALL_DIR } elseif ($env:LOCALAPPDATA) { Join-Path $env:LOCALAPPDATA "Programs\agents-notifier" } else { "" })
)

$ErrorActionPreference = "Stop"

if (-not $InstallDir) {
    throw "LOCALAPPDATA is not set. Set AGENTS_NOTIFIER_INSTALL_DIR to choose an install directory."
}

switch ($env:PROCESSOR_ARCHITECTURE) {
    "AMD64" { $Target = "x86_64-pc-windows-msvc" }
    default { throw "Unsupported Windows architecture: $env:PROCESSOR_ARCHITECTURE" }
}

$Archive = "agents-notifier-$Target.zip"
$BaseUrl = "https://github.com/$Repo/releases/latest/download"
$TempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("agents-notifier-" + [System.Guid]::NewGuid().ToString("N"))

New-Item -ItemType Directory -Path $TempDir | Out-Null

try {
    Write-Host "Downloading Agents Notifier for $Target..."
    $ArchivePath = Join-Path $TempDir $Archive
    $ChecksumPath = Join-Path $TempDir "$Archive.sha256"
    Invoke-WebRequest -Uri "$BaseUrl/$Archive" -OutFile $ArchivePath
    Invoke-WebRequest -Uri "$BaseUrl/$Archive.sha256" -OutFile $ChecksumPath

    $ExpectedHash = (Get-Content $ChecksumPath -Raw).Trim().Split(" ", [System.StringSplitOptions]::RemoveEmptyEntries)[0].ToLowerInvariant()
    $ActualHash = (Get-FileHash -Algorithm SHA256 $ArchivePath).Hash.ToLowerInvariant()
    if ($ExpectedHash -ne $ActualHash) {
        throw "SHA256 verification failed for $Archive"
    }

    Expand-Archive -Path $ArchivePath -DestinationPath $TempDir -Force
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    Copy-Item -Path (Join-Path $TempDir "agents-notifier.exe") -Destination (Join-Path $InstallDir "agents-notifier.exe") -Force

    Write-Host "Installed: $(Join-Path $InstallDir "agents-notifier.exe")"
    Write-Host ""
    Write-Host "Add this directory to PATH if agents-notifier is not found:"
    Write-Host "  $InstallDir"
    Write-Host ""
    Write-Host "Next:"
    Write-Host "  agents-notifier setup"
}
finally {
    Remove-Item -Recurse -Force $TempDir -ErrorAction SilentlyContinue
}
