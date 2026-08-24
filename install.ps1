[CmdletBinding()]
param(
    [Parameter()]
    [string] $Version,

    [Parameter()]
    [string] $BinDir,

    [Parameter()]
    [switch] $NoModifyPath
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = "Stop"
[Net.ServicePointManager]::SecurityProtocol =
    [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

$Repository = "pulkitxm/Gitbot"
$ReleasesUrl = "https://github.com/$Repository/releases"

function Write-Info {
    param([string] $Message)
    Write-Output "info: $Message"
}

function Test-PathEntry {
    param(
        [string] $PathValue,
        [string] $Entry
    )

    if ([string]::IsNullOrWhiteSpace($PathValue)) {
        return $false
    }

    $normalizedEntry = $Entry.TrimEnd([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar)
    foreach ($candidate in $PathValue.Split([IO.Path]::PathSeparator)) {
        $normalizedCandidate = $candidate.Trim().TrimEnd([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar)
        if ($normalizedCandidate.Equals($normalizedEntry, [StringComparison]::OrdinalIgnoreCase)) {
            return $true
        }
    }
    return $false
}

function Invoke-Download {
    param(
        [string] $Uri,
        [string] $OutFile
    )

    try {
        Invoke-WebRequest -Uri $Uri -OutFile $OutFile -UseBasicParsing
    }
    catch {
        throw "failed to download ${Uri}: $($_.Exception.Message)"
    }
}

function Install-Gitbot {
    param(
        [string] $RequestedVersion,
        [string] $RequestedBinDir,
        [switch] $SkipPathUpdate
    )

    if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
        throw "install.ps1 only supports Windows; use install.sh on Linux or macOS"
    }

    if ([string]::IsNullOrWhiteSpace($RequestedVersion)) {
        $RequestedVersion = if ([string]::IsNullOrWhiteSpace($env:GITBOT_VERSION)) {
            "latest"
        }
        else {
            $env:GITBOT_VERSION
        }
    }

    if ([string]::IsNullOrWhiteSpace($RequestedBinDir)) {
        if (-not [string]::IsNullOrWhiteSpace($env:GITBOT_INSTALL_DIR)) {
            $RequestedBinDir = $env:GITBOT_INSTALL_DIR
        }
        elseif (-not [string]::IsNullOrWhiteSpace($env:LOCALAPPDATA)) {
            $RequestedBinDir = Join-Path $env:LOCALAPPDATA "Programs\Gitbot\bin"
        }
        elseif (-not [string]::IsNullOrWhiteSpace($HOME)) {
            $RequestedBinDir = Join-Path $HOME ".local\bin"
        }
        else {
            throw "could not determine an installation directory; set GITBOT_INSTALL_DIR"
        }
    }

    $architecture = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
    switch ($architecture) {
        "X64" { $asset = "gitbot-windows-x86_64.exe" }
        "Arm64" {
            $asset = "gitbot-windows-x86_64.exe"
            Write-Warning "a native Windows ARM64 build is not available; installing the x64 build"
        }
        default { throw "Gitbot does not publish a Windows release for architecture '$architecture'" }
    }

    if ($RequestedVersion -eq "latest") {
        $releaseUrl = "$ReleasesUrl/latest/download"
        $versionLabel = "latest"
    }
    else {
        $releaseTag = if ($RequestedVersion.StartsWith("v")) { $RequestedVersion } else { "v$RequestedVersion" }
        if ($releaseTag -notmatch '^v[0-9][0-9A-Za-z._+-]*$') {
            throw "invalid release version: $RequestedVersion"
        }
        $releaseUrl = "$ReleasesUrl/download/$releaseTag"
        $versionLabel = $releaseTag
    }

    $tempDir = Join-Path ([IO.Path]::GetTempPath()) ("gitbot-install-" + [Guid]::NewGuid().ToString("N"))
    $downloadPath = Join-Path $tempDir $asset
    $checksumsPath = Join-Path $tempDir "SHA256SUMS"

    New-Item -ItemType Directory -Path $tempDir | Out-Null
    try {
        Write-Info "detected Windows $architecture"
        Write-Info "downloading Gitbot $versionLabel"
        Invoke-Download -Uri "$releaseUrl/SHA256SUMS" -OutFile $checksumsPath
        Invoke-Download -Uri "$releaseUrl/$asset" -OutFile $downloadPath

        $escapedAsset = [Regex]::Escape($asset)
        $checksumPattern = "^(?<hash>[0-9A-Fa-f]{64})\s+\*?(?:dist/)?${escapedAsset}$"
        $checksumMatch = $null
        foreach ($line in Get-Content -LiteralPath $checksumsPath) {
            $match = [Regex]::Match($line.Trim(), $checksumPattern)
            if ($match.Success) {
                $checksumMatch = $match
                break
            }
        }
        if ($null -eq $checksumMatch) {
            throw "the release checksum for $asset is missing or invalid"
        }

        $expectedChecksum = $checksumMatch.Groups["hash"].Value
        $actualChecksum = (Get-FileHash -LiteralPath $downloadPath -Algorithm SHA256).Hash
        if (-not $expectedChecksum.Equals($actualChecksum, [StringComparison]::OrdinalIgnoreCase)) {
            throw "checksum verification failed for $asset"
        }
        Write-Info "verified SHA-256 checksum"

        New-Item -ItemType Directory -Force -Path $RequestedBinDir | Out-Null
        $destination = Join-Path $RequestedBinDir "gitbot.exe"
        Move-Item -LiteralPath $downloadPath -Destination $destination -Force
        Write-Output "`nGitbot was installed to $destination"

        $processPath = [Environment]::GetEnvironmentVariable("Path", "Process")
        if (-not (Test-PathEntry -PathValue $processPath -Entry $RequestedBinDir)) {
            if ($SkipPathUpdate.IsPresent) {
                Write-Warning "$RequestedBinDir is not on PATH"
            }
            else {
                $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
                if (-not (Test-PathEntry -PathValue $userPath -Entry $RequestedBinDir)) {
                    $updatedPath = if ([string]::IsNullOrWhiteSpace($userPath)) {
                        $RequestedBinDir
                    }
                    else {
                        "$RequestedBinDir$([IO.Path]::PathSeparator)$userPath"
                    }
                    [Environment]::SetEnvironmentVariable("Path", $updatedPath, "User")
                    Write-Info "added $RequestedBinDir to your user PATH"
                }
                Write-Warning "restart your terminal before running gitbot"
            }
        }
    }
    finally {
        Remove-Item -LiteralPath $tempDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Install-Gitbot -RequestedVersion $Version -RequestedBinDir $BinDir -SkipPathUpdate:$NoModifyPath
