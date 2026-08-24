$ErrorActionPreference = "Stop"
Set-StrictMode -Version 3.0

$Root = Split-Path -Parent $PSScriptRoot
$Installer = Join-Path $Root "install.ps1"
$TestRoot = Join-Path ([IO.Path]::GetTempPath()) ("pukbot-installer-tests-" + [Guid]::NewGuid().ToString("N"))
$Fixtures = Join-Path $TestRoot "fixtures"
$DownloadsLog = Join-Path $TestRoot "downloads.log"

function Assert-Equal {
    param(
        [object] $Expected,
        [object] $Actual,
        [string] $Message
    )
    if ($Expected -ne $Actual) {
        throw "${Message}: expected '$Expected', got '$Actual'"
    }
}

function Assert-Contains {
    param(
        [string] $Needle,
        [string] $Path
    )
    if (-not (Select-String -LiteralPath $Path -SimpleMatch $Needle -Quiet)) {
        throw "expected '$Needle' in $Path"
    }
}

function Set-ReleaseFixture {
    param(
        [string] $Contents,
        [switch] $InvalidChecksum
    )

    $asset = "pukbot-windows-x86_64.exe"
    $assetPath = Join-Path $Fixtures $asset
    [IO.File]::WriteAllText($assetPath, $Contents)
    $hash = if ($InvalidChecksum) { "0" * 64 } else { (Get-FileHash -LiteralPath $assetPath -Algorithm SHA256).Hash }
    [IO.File]::WriteAllText((Join-Path $Fixtures "SHA256SUMS"), "$hash  $asset`n")
}

New-Item -ItemType Directory -Path $Fixtures | Out-Null
[IO.File]::WriteAllText($DownloadsLog, "")

function global:Invoke-WebRequest {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $Uri,
        [Parameter(Mandatory)]
        [string] $OutFile,
        [Parameter()]
        [switch] $UseBasicParsing
    )

    Add-Content -LiteralPath $global:PukbotDownloadsLog -Value $Uri
    $asset = [IO.Path]::GetFileName(([Uri] $Uri).AbsolutePath)
    Copy-Item -LiteralPath (Join-Path $global:PukbotFixtures $asset) -Destination $OutFile
}

$global:PukbotFixtures = $Fixtures
$global:PukbotDownloadsLog = $DownloadsLog

try {
    Write-Host "test: installs and verifies a pinned Windows release"
    $binDir = Join-Path $TestRoot "successful-install\bin"
    Set-ReleaseFixture -Contents "windows binary"
    & $Installer -Version "1.2.3" -BinDir $binDir -NoModifyPath *> (Join-Path $TestRoot "successful-install.log")
    Assert-Equal -Expected "windows binary" -Actual ([IO.File]::ReadAllText((Join-Path $binDir "pukbot.exe"))) -Message "installed binary"
    Assert-Contains -Needle "https://github.com/pulkitxm/pukbot/releases/download/v1.2.3/pukbot-windows-x86_64.exe" -Path $DownloadsLog
    Assert-Contains -Needle "verified SHA-256 checksum" -Path (Join-Path $TestRoot "successful-install.log")

    Write-Host "test: rejects a checksum mismatch"
    $binDir = Join-Path $TestRoot "bad-checksum\bin"
    Set-ReleaseFixture -Contents "tampered binary" -InvalidChecksum
    $failedAsExpected = $false
    try {
        & $Installer -Version "latest" -BinDir $binDir -NoModifyPath
    }
    catch {
        if ($_.Exception.Message -like "*checksum verification failed*") {
            $failedAsExpected = $true
        }
        else {
            throw
        }
    }
    if (-not $failedAsExpected) {
        throw "checksum mismatch unexpectedly succeeded"
    }

    Write-Host "All PowerShell installer tests passed."
}
finally {
    Remove-Item Function:\Invoke-WebRequest -Force -ErrorAction SilentlyContinue
    Remove-Variable PukbotFixtures -Scope Global -ErrorAction SilentlyContinue
    Remove-Variable PukbotDownloadsLog -Scope Global -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $TestRoot -Recurse -Force -ErrorAction SilentlyContinue
}
