[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("Info", "Extract", "Verify")]
    [string]$Action,

    [Parameter(Mandatory = $true)]
    [string]$Archive,

    [Parameter(Mandatory = $true)]
    [string]$RgxExe,

    [switch]$NoPause
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$resolvedArchive = (Resolve-Path -LiteralPath $Archive -ErrorAction Stop).Path
if (-not (Test-Path -LiteralPath $resolvedArchive -PathType Leaf)) {
    throw "RGX archive not found: $Archive"
}

$resolvedRgxExe = (Resolve-Path -LiteralPath $RgxExe -ErrorAction Stop).Path
if (-not (Test-Path -LiteralPath $resolvedRgxExe -PathType Leaf)) {
    throw "RGX executable not found: $RgxExe"
}

try {
    $Host.UI.RawUI.WindowTitle = "RGX - $Action"
}
catch {
    # Some hosts do not expose RawUI. The shell action still works without a title.
}

$script:exitCode = 0

function Invoke-Rgx {
    param(
        [Parameter(Mandatory = $true)]
        [string[]]$Arguments
    )

    # Array splatting keeps every path as a distinct argument. This is important for
    # Explorer paths containing spaces, ampersands, parentheses, umlauts, and other
    # characters that are otherwise easy to break through string-built command lines.
    & $resolvedRgxExe @Arguments
    $script:exitCode = $LASTEXITCODE
    if ($script:exitCode -ne 0) {
        throw "RGX exited with code $script:exitCode."
    }
}

try {
    switch ($Action) {
        "Info" {
            Invoke-Rgx -Arguments @("info", $resolvedArchive)
        }
        "Verify" {
            Invoke-Rgx -Arguments @("verify", $resolvedArchive)
        }
        "Extract" {
            $archiveDirectory = [IO.Path]::GetDirectoryName($resolvedArchive)
            $archiveName = [IO.Path]::GetFileNameWithoutExtension($resolvedArchive)
            if ([string]::IsNullOrWhiteSpace($archiveName)) {
                throw "Cannot derive an extraction directory from: $resolvedArchive"
            }

            $baseOutput = [IO.Path]::Combine($archiveDirectory, $archiveName)
            $output = $baseOutput
            $suffix = 2
            while (Test-Path -LiteralPath $output) {
                $output = "{0}-{1}" -f $baseOutput, $suffix
                $suffix++
            }

            Write-Host "Extracting to: $output"
            Invoke-Rgx -Arguments @("extract", $resolvedArchive, $output)
        }
    }
}
catch {
    if ($script:exitCode -eq 0) {
        $script:exitCode = 1
    }
    Write-Host ""
    Write-Host "RGX action failed: $($_.Exception.Message)" -ForegroundColor Red
}

Write-Host ""
if ($script:exitCode -eq 0) {
    Write-Host "RGX action completed successfully." -ForegroundColor Green
}
else {
    Write-Host "RGX exited with code $script:exitCode." -ForegroundColor Red
}

if (-not $NoPause) {
    try {
        Read-Host "Press Enter to close" | Out-Null
    }
    catch {
        # Non-interactive hosts may not provide stdin.
    }
}

exit $script:exitCode
