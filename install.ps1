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

function Test-AgentServiceRunning {
    $output = & schtasks.exe /Query /TN "\AgentsNotifier" /FO LIST /V 2>$null | Out-String
    if ($LASTEXITCODE -ne 0) {
        return $false
    }

    return $output -match '(?im)^\s*Status:\s*Running\s*$'
}

function Get-AgentServiceConfigPath {
    if (-not $env:USERPROFILE) {
        throw "USERPROFILE is not set."
    }

    $metadataPath = Join-Path $env:USERPROFILE "AppData\Local\agents-notifier\service.json"
    if (Test-Path $metadataPath) {
        $metadata = Get-Content -Path $metadataPath -Raw | ConvertFrom-Json
        if ($metadata.config_path) {
            return [string]$metadata.config_path
        }
        throw "Could not read config_path from service metadata: $metadataPath"
    }

    return (Join-Path $env:USERPROFILE "AppData\Roaming\agents-notifier\config.toml")
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
$RestartServiceAfterInstall = Test-AgentServiceRunning

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
    if ($RestartServiceAfterInstall -and (Test-Path $DestinationPath)) {
        & $DestinationPath stop
        if ($LASTEXITCODE -ne 0) {
            throw "Existing Agents Notifier stop command exited with code $LASTEXITCODE."
        }
    }
    Copy-Item -Path (Join-Path $TempDir "agents-notifier.exe") -Destination $DestinationPath -Force
    Set-Content -Path (Join-Path $InstallDir ".agents-notifier-install-method") -Value "script" -NoNewline

    Write-Host "Installed: $DestinationPath"
    if ($RestartServiceAfterInstall) {
        $ConfigPath = Get-AgentServiceConfigPath
        Write-Host "Restarting existing Agents Notifier service..."
        & $DestinationPath stop
        if ($LASTEXITCODE -ne 0) {
            throw "Agents Notifier stop command exited with code $LASTEXITCODE."
        }
        & $DestinationPath start --config $ConfigPath
        if ($LASTEXITCODE -ne 0) {
            throw "Agents Notifier start command exited with code $LASTEXITCODE."
        }
    }
    Write-Host ""
    Write-Host "Add this directory to PATH if agents-notifier is not found:"
    Write-Host "  $InstallDir"
    Write-Host ""
    if ($RestartServiceAfterInstall) {
        Write-Host "Service restarted with the installed version."
    }
    else {
        Write-Host "Next:"
        Write-Host "  agents-notifier setup"
    }
}
finally {
    Remove-Item -Recurse -Force $TempDir -ErrorAction SilentlyContinue
}
