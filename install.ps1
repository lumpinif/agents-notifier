param(
    [string]$Repo = $(if ($env:AGENTS_NOTIFIER_REPO) { $env:AGENTS_NOTIFIER_REPO } else { "lumpinif/agents-notifier" }),
    [string]$InstallDir = $(if ($env:AGENTS_NOTIFIER_INSTALL_DIR) { $env:AGENTS_NOTIFIER_INSTALL_DIR } elseif ($env:LOCALAPPDATA) { Join-Path $env:LOCALAPPDATA "Programs\agents-notifier" } else { "" }),
    [string]$Version = $(if ($env:AGENTS_NOTIFIER_VERSION) { $env:AGENTS_NOTIFIER_VERSION } else { "latest" })
)

$ErrorActionPreference = "Stop"

if (-not $InstallDir) {
    throw "LOCALAPPDATA is not set. Set AGENTS_NOTIFIER_INSTALL_DIR to choose an install directory."
}

if ($Version -notmatch '^(latest|v\d+\.\d+\.\d+)$') {
    throw "AGENTS_NOTIFIER_VERSION must be latest or a vX.Y.Z tag; got: $Version"
}

switch ($env:PROCESSOR_ARCHITECTURE) {
    "AMD64" { $Target = "x86_64-pc-windows-msvc" }
    default { throw "Unsupported Windows architecture: $env:PROCESSOR_ARCHITECTURE" }
}

$Archive = "agents-notifier-$Target.zip"
if ($Version -eq "latest") {
    $BaseUrl = "https://github.com/$Repo/releases/latest/download"
}
else {
    $BaseUrl = "https://github.com/$Repo/releases/download/$Version"
}
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
    $DestinationPath = Join-Path $InstallDir "agents-notifier.exe"
    if (Test-Path $DestinationPath) {
        try {
            & $DestinationPath stop
            if ($LASTEXITCODE -ne 0) {
                Write-Host "Existing Agents Notifier stop command exited with code $LASTEXITCODE. Continuing with install..."
            }
        }
        catch {
            Write-Host "Could not stop existing Agents Notifier before install: $($_.Exception.Message)"
        }
    }
    Copy-Item -Path (Join-Path $TempDir "agents-notifier.exe") -Destination $DestinationPath -Force
    Set-Content -Path (Join-Path $InstallDir ".agents-notifier-install-method") -Value "script" -NoNewline

    Write-Host "Installed: $DestinationPath"
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
