[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("Info", "Extract", "Verify")]
    [string]$Action,

    [Parameter(Mandatory = $true)]
    [string]$Archive,

    [Parameter(Mandatory = $true)]
    [string]$RgxExe
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not (Test-Path -LiteralPath $Archive -PathType Leaf)) {
    throw "RGX archive not found: $Archive"
}
if (-not (Test-Path -LiteralPath $RgxExe -PathType Leaf)) {
    throw "RGX executable not found: $RgxExe"
}

try {
    $Host.UI.RawUI.WindowTitle = "RGX - $Action"
}
catch {
    # Some hosts do not expose RawUI. The shell action still works without a title.
}

$exitCode = 0
try {
    switch ($Action) {
        "Info" {
            & $RgxExe info $Archive
            $exitCode = $LASTEXITCODE
        }
        "Verify" {
            & $RgxExe verify $Archive
            $exitCode = $LASTEXITCODE
        }
        "Extract" {
            $archiveDirectory = Split-Path -LiteralPath $Archive -Parent
            $archiveName = [IO.Path]::GetFileNameWithoutExtension($Archive)
            $baseOutput = Join-Path $archiveDirectory $archiveName
            $output = $baseOutput
            $suffix = 2
            while (Test-Path -LiteralPath $output) {
                $output = "{0}-{1}" -f $baseOutput, $suffix
                $suffix++
            }

            Write-Host "Extracting to: $output"
            & $RgxExe extract $Archive $output
            $exitCode = $LASTEXITCODE
        }
    }
}
catch {
    Write-Host ""
    Write-Host "RGX action failed: $($_.Exception.Message)" -ForegroundColor Red
    $exitCode = 1
}

Write-Host ""
if ($exitCode -eq 0) {
    Write-Host "RGX action completed successfully." -ForegroundColor Green
}
else {
    Write-Host "RGX exited with code $exitCode." -ForegroundColor Red
}

Read-Host "Press Enter to close" | Out-Null
exit $exitCode
