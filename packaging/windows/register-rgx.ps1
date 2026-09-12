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

    foreach ($name in @("rgx.exe", "rgx")) {
        $command = Get-Command $name -ErrorAction SilentlyContinue
        if ($command -and $command.Source) {
            return $command.Source
        }
    }

    $repositoryRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\.."))
    foreach ($candidate in @(
        (Join-Path $repositoryRoot "target\release\rgx.exe"),
        (Join-Path $repositoryRoot "target\debug\rgx.exe"),
        (Join-Path $PSScriptRoot "rgx.exe")
    )) {
        if (Test-Path -LiteralPath $candidate -PathType Leaf) {
            return [IO.Path]::GetFullPath($candidate)
        }
    }

    throw "rgx.exe was not found. Build RGX first or pass -RgxExe <path-to-rgx.exe>."
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

        Remove-RegistryTree -Path (Join-Path $ExtensionKey "DefaultIcon")

        $openWithRegistryKey = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey("Software\Classes\.rgx\OpenWithProgids", $true)
        if ($null -ne $openWithRegistryKey) {
            try {
                $openWithRegistryKey.DeleteValue($ProgId, $false)
            }
            finally {
                $openWithRegistryKey.Dispose()
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
    $IconSource = Join-Path $PSScriptRoot "rgx-file-icon.ico"
}
$resolvedIconSource = (Resolve-Path -LiteralPath $IconSource -ErrorAction Stop).Path
if ([IO.Path]::GetExtension($resolvedIconSource) -ne ".ico") {
    throw "RGX icon source must be an .ico file: $resolvedIconSource"
}

$helperSource = Join-Path $PSScriptRoot "rgx-shell.ps1"
if (-not (Test-Path -LiteralPath $helperSource -PathType Leaf)) {
    throw "RGX shell helper not found: $helperSource"
}

New-Item -ItemType Directory -Path $StateDirectory -Force | Out-Null
Copy-Item -LiteralPath $helperSource -Destination $InstalledHelper -Force
Copy-Item -LiteralPath $resolvedIconSource -Destination $InstalledIcon -Force

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

foreach ($verb in @(
    @{ Key = "RGX.Info"; Text = "Open with RGX"; Command = $openCommand },
    @{ Key = "RGX.Extract"; Text = "Extract with RGX"; Command = $extractCommand },
    @{ Key = "RGX.Verify"; Text = "Verify with RGX"; Command = $verifyCommand }
)) {
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
