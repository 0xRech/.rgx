[CmdletBinding()]
param(
    [string]$RgxExe,
    [string]$IconSource,
    [switch]$Unregister
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$ProgId = "RGX.Archive"
$ClassesRoot = "HKCU:\Software\Classes"
$ExtensionKey = Join-Path $ClassesRoot ".rgx"
$ProgIdKey = Join-Path $ClassesRoot $ProgId
$SystemAssociationKey = Join-Path $ClassesRoot "SystemFileAssociations\.rgx"
$ApplicationKey = Join-Path $ClassesRoot "Applications\rgx.exe"
$StateDirectory = Join-Path $env:LOCALAPPDATA "RGX\Shell"
$InstalledHelper = Join-Path $StateDirectory "rgx-shell.ps1"
$InstalledIcon = Join-Path $StateDirectory "rgx-file.ico"

function Ensure-RegistryKey {
    param([Parameter(Mandatory = $true)][string]$Path)

    if (-not (Test-Path -LiteralPath $Path)) {
        New-Item -Path $Path -Force | Out-Null
    }
}

function Set-DefaultValue {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Value
    )

    Ensure-RegistryKey -Path $Path
    Set-Item -Path $Path -Value $Value
}

function Set-StringValue {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][AllowEmptyString()][string]$Value
    )

    Ensure-RegistryKey -Path $Path
    New-ItemProperty -Path $Path -Name $Name -PropertyType String -Value $Value -Force | Out-Null
}

function Remove-RegistryTree {
    param([Parameter(Mandatory = $true)][string]$Path)

    if (Test-Path -LiteralPath $Path) {
        Remove-Item -LiteralPath $Path -Recurse -Force
    }
}

function Refresh-ExplorerAssociations {
    if (-not ("RGX.ShellRefresh" -as [type])) {
        Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
namespace RGX {
    public static class ShellRefresh {
        [DllImport("shell32.dll")]
        public static extern void SHChangeNotify(uint eventId, uint flags, IntPtr item1, IntPtr item2);
    }
}
"@
    }

    [RGX.ShellRefresh]::SHChangeNotify(0x08000000, 0, [IntPtr]::Zero, [IntPtr]::Zero)
}

function Resolve-RgxExecutable {
    param([string]$RequestedPath)

    if ($RequestedPath) {
        $resolved = Resolve-Path -LiteralPath $RequestedPath -ErrorAction Stop
        if (-not (Test-Path -LiteralPath $resolved.Path -PathType Leaf)) {
            throw "RGX executable not found: $RequestedPath"
        }
        return $resolved.Path
    }

    $command = Get-Command rgx.exe -ErrorAction SilentlyContinue
    if (-not $command) {
        $command = Get-Command rgx -ErrorAction SilentlyContinue
    }
    if ($command -and $command.Source) {
        return $command.Source
    }

    $repositoryRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\.."))
    $candidates = @(
        (Join-Path $repositoryRoot "target\release\rgx.exe"),
        (Join-Path $repositoryRoot "target\debug\rgx.exe"),
        (Join-Path $PSScriptRoot "rgx.exe")
    )

    foreach ($candidate in $candidates) {
        if (Test-Path -LiteralPath $candidate -PathType Leaf) {
            return [IO.Path]::GetFullPath($candidate)
        }
    }

    throw "rgx.exe was not found. Build RGX first or pass -RgxExe <path-to-rgx.exe>."
}

function Read-PngUInt32BigEndian {
    param(
        [Parameter(Mandatory = $true)][byte[]]$Bytes,
        [Parameter(Mandatory = $true)][int]$Offset
    )

    return ([int64]$Bytes[$Offset] * 16777216) +
           ([int64]$Bytes[$Offset + 1] * 65536) +
           ([int64]$Bytes[$Offset + 2] * 256) +
           [int64]$Bytes[$Offset + 3]
}

function Convert-PngToIco {
    param(
        [Parameter(Mandatory = $true)][string]$Source,
        [Parameter(Mandatory = $true)][string]$Destination
    )

    $png = [IO.File]::ReadAllBytes($Source)
    $signature = [byte[]](0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A)
    if ($png.Length -lt 24) {
        throw "RGX icon source is not a valid PNG: $Source"
    }
    for ($i = 0; $i -lt $signature.Length; $i++) {
        if ($png[$i] -ne $signature[$i]) {
            throw "RGX icon source is not a valid PNG: $Source"
        }
    }

    $width = Read-PngUInt32BigEndian -Bytes $png -Offset 16
    $height = Read-PngUInt32BigEndian -Bytes $png -Offset 20
    if ($width -lt 1 -or $width -gt 256 -or $height -lt 1 -or $height -gt 256) {
        throw "RGX PNG icon must be between 1x1 and 256x256 pixels; got ${width}x${height}."
    }

    $widthByte = if ($width -eq 256) { [byte]0 } else { [byte]$width }
    $heightByte = if ($height -eq 256) { [byte]0 } else { [byte]$height }

    $stream = [IO.File]::Open($Destination, [IO.FileMode]::Create, [IO.FileAccess]::Write, [IO.FileShare]::None)
    $writer = [IO.BinaryWriter]::new($stream)
    try {
        $writer.Write([UInt16]0)
        $writer.Write([UInt16]1)
        $writer.Write([UInt16]1)
        $writer.Write($widthByte)
        $writer.Write($heightByte)
        $writer.Write([byte]0)
        $writer.Write([byte]0)
        $writer.Write([UInt16]1)
        $writer.Write([UInt16]32)
        $writer.Write([UInt32]$png.Length)
        $writer.Write([UInt32]22)
        $writer.Write($png)
    }
    finally {
        $writer.Dispose()
        $stream.Dispose()
    }
}

function Remove-RgxRegistration {
    Remove-RegistryTree -Path $ProgIdKey
    Remove-RegistryTree -Path $ApplicationKey

    foreach ($verb in @("RGX.Info", "RGX.Extract", "RGX.Verify")) {
        Remove-RegistryTree -Path (Join-Path $SystemAssociationKey "shell\$verb")
    }

    if (Test-Path -LiteralPath $ExtensionKey) {
        $extension = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey("Software\Classes\.rgx", $true)
        if ($null -ne $extension) {
            try {
                if ($extension.GetValue("") -eq $ProgId) {
                    $extension.DeleteValue("", $false)
                }
                foreach ($name in @("Content Type", "PerceivedType")) {
                    $extension.DeleteValue($name, $false)
                }
            }
            finally {
                $extension.Dispose()
            }
        }

        $openWithRegistryKey = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey("Software\Classes\.rgx\OpenWithProgids", $true)
        if ($null -ne $openWithRegistryKey) {
            try {
                $openWithRegistryKey.DeleteValue($ProgId, $false)
            }
            finally {
                $openWithRegistryKey.Dispose()
            }
        }

        $extensionIconKey = Join-Path $ExtensionKey "DefaultIcon"
        if (Test-Path -LiteralPath $extensionIconKey) {
            $iconValue = (Get-Item -LiteralPath $extensionIconKey).GetValue("")
            if ($iconValue -eq ('"{0}",0' -f $InstalledIcon)) {
                Remove-RegistryTree -Path $extensionIconKey
            }
        }
    }

    if (Test-Path -LiteralPath $StateDirectory) {
        Remove-Item -LiteralPath $StateDirectory -Recurse -Force
    }

    Refresh-ExplorerAssociations
    Write-Host "RGX Windows file association removed."
}

if ($Unregister) {
    Remove-RgxRegistration
    exit 0
}

$resolvedExe = Resolve-RgxExecutable -RequestedPath $RgxExe
if (-not $IconSource) {
    $IconSource = Join-Path $PSScriptRoot "rgx-file-icon.png"
}
$resolvedIconSource = (Resolve-Path -LiteralPath $IconSource -ErrorAction Stop).Path
$helperSource = Join-Path $PSScriptRoot "rgx-shell.ps1"
if (-not (Test-Path -LiteralPath $helperSource -PathType Leaf)) {
    throw "RGX shell helper not found: $helperSource"
}

New-Item -ItemType Directory -Path $StateDirectory -Force | Out-Null
Copy-Item -LiteralPath $helperSource -Destination $InstalledHelper -Force
Convert-PngToIco -Source $resolvedIconSource -Destination $InstalledIcon

$windowsPowerShell = Join-Path $env:SystemRoot "System32\WindowsPowerShell\v1.0\powershell.exe"
if (-not (Test-Path -LiteralPath $windowsPowerShell -PathType Leaf)) {
    $windowsPowerShell = (Get-Command powershell.exe -ErrorAction Stop).Source
}

$iconValue = '"{0}",0' -f $InstalledIcon
$openCommand = '"{0}" -NoProfile -ExecutionPolicy Bypass -File "{1}" -Action Info -Archive "%1" -RgxExe "{2}"' -f $windowsPowerShell, $InstalledHelper, $resolvedExe
$extractCommand = '"{0}" -NoProfile -ExecutionPolicy Bypass -File "{1}" -Action Extract -Archive "%1" -RgxExe "{2}"' -f $windowsPowerShell, $InstalledHelper, $resolvedExe
$verifyCommand = '"{0}" -NoProfile -ExecutionPolicy Bypass -File "{1}" -Action Verify -Archive "%1" -RgxExe "{2}"' -f $windowsPowerShell, $InstalledHelper, $resolvedExe

Set-DefaultValue -Path $ExtensionKey -Value $ProgId
Set-StringValue -Path $ExtensionKey -Name "Content Type" -Value "application/x-rgx"
Set-StringValue -Path $ExtensionKey -Name "PerceivedType" -Value "compressed"
Set-DefaultValue -Path (Join-Path $ExtensionKey "DefaultIcon") -Value $iconValue
Set-StringValue -Path (Join-Path $ExtensionKey "OpenWithProgids") -Name $ProgId -Value ""

Set-DefaultValue -Path $ProgIdKey -Value "RGX Archive"
Set-StringValue -Path $ProgIdKey -Name "FriendlyTypeName" -Value "RGX Archive"
Set-DefaultValue -Path (Join-Path $ProgIdKey "DefaultIcon") -Value $iconValue
Set-DefaultValue -Path (Join-Path $ProgIdKey "shell") -Value "open"
Set-DefaultValue -Path (Join-Path $ProgIdKey "shell\open") -Value "Open with RGX"
Set-StringValue -Path (Join-Path $ProgIdKey "shell\open") -Name "Icon" -Value $InstalledIcon
Set-DefaultValue -Path (Join-Path $ProgIdKey "shell\open\command") -Value $openCommand

Set-StringValue -Path $ApplicationKey -Name "FriendlyAppName" -Value "RGX"
Set-StringValue -Path (Join-Path $ApplicationKey "SupportedTypes") -Name ".rgx" -Value ""
Set-DefaultValue -Path (Join-Path $ApplicationKey "shell\open\command") -Value $openCommand

$verbs = @(
    @{ Key = "RGX.Info"; Text = "Open with RGX"; Command = $openCommand },
    @{ Key = "RGX.Extract"; Text = "Extract with RGX"; Command = $extractCommand },
    @{ Key = "RGX.Verify"; Text = "Verify with RGX"; Command = $verifyCommand }
)
foreach ($verb in $verbs) {
    $verbKey = Join-Path $SystemAssociationKey ("shell\" + $verb.Key)
    Set-StringValue -Path $verbKey -Name "MUIVerb" -Value $verb.Text
    Set-StringValue -Path $verbKey -Name "Icon" -Value $InstalledIcon
    Set-DefaultValue -Path (Join-Path $verbKey "command") -Value $verb.Command
}

Refresh-ExplorerAssociations

Write-Host "RGX Windows integration registered for the current user."
Write-Host "Executable: $resolvedExe"
Write-Host "File type:   .rgx -> $ProgId"
Write-Host "Icon:        $InstalledIcon"
Write-Host "Actions:     Open, Extract, Verify"
Write-Host "No administrator privileges were required."
