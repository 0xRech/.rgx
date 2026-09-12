[CmdletBinding()]
param(
    [switch]$CheckOnly,
    [switch]$IncludePrerelease
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$Repository = "0xRech/.rgx"
$RgxExe = Join-Path $PSScriptRoot "rgx.exe"
$ReleasesApi = "https://api.github.com/repos/$Repository/releases?per_page=30"

function Get-InstalledVersion {
    if (-not (Test-Path -LiteralPath $RgxExe -PathType Leaf)) {
        throw "RGX executable not found next to updater: $RgxExe"
    }

    $versionOutput = (& $RgxExe --version 2>&1 | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or $versionOutput -notmatch '^rgx\s+(?<version>\S+)$') {
        throw "Could not determine the installed RGX version."
    }
    return $Matches.version
}

function Normalize-VersionTag {
    param([Parameter(Mandatory = $true)][string]$Value)
    return ($Value.Trim() -replace '^v', '')
}

[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$headers = @{
    "Accept" = "application/vnd.github+json"
    "User-Agent" = "RGX-Windows-Updater"
    "X-GitHub-Api-Version" = "2022-11-28"
}

$currentVersion = Get-InstalledVersion
$releases = @(Invoke-RestMethod -Uri $ReleasesApi -Headers $headers -Method Get)
$allowPrerelease = $IncludePrerelease -or $currentVersion.Contains('-')
$eligible = @($releases | Where-Object {
    -not $_.draft -and ($allowPrerelease -or -not $_.prerelease)
})

if ($eligible.Count -eq 0) {
    throw "No eligible RGX release was found."
}

$release = $eligible[0]
$latestVersion = Normalize-VersionTag -Value ([string]$release.tag_name)

Write-Host "Installed RGX: $currentVersion"
Write-Host "Latest RGX:    $latestVersion"

if ((Normalize-VersionTag -Value $currentVersion) -eq $latestVersion) {
    Write-Host "RGX is already up to date."
    exit 0
}

Write-Host "Update available: $currentVersion -> $latestVersion"
Write-Host "The RGX Windows installer upgrades an existing installation in place."

if (-not $CheckOnly) {
    $answer = Read-Host "Open the verified GitHub release page now? [Y/n]"
    if (-not $answer -or $answer -match '^(?i:y|yes|j|ja)$') {
        Start-Process ([string]$release.html_url)
    }
}
