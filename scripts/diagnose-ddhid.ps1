[CmdletBinding()]
param(
    # Optional. Normal users do not need this.
    # The script auto-detects the default FlairBloom install location first.
    # This can point to the app root, resources folder, or ddhid-driver folder.
    [string]$DriverDir = "",

    # Optional. Defaults to Desktop\ddhid-diagnose-<timestamp>.txt.
    [string]$OutFile = "",

    # Internal app mode. The app writes its structured diagnosis separately and
    # uses this script only for Windows-side evidence that is hard to collect in Rust.
    [switch]$EvidenceOnly
)

$ErrorActionPreference = "Continue"
$script:Lines = New-Object System.Collections.Generic.List[string]

function Add-Line {
    param([string]$Text = "")
    [void]$script:Lines.Add($Text)
}

function Add-Section {
    param([string]$Name)
    Add-Line ""
    Add-Line ("==== {0} ====" -f $Name)
}

function Add-CommandOutput {
    param(
        [string]$Title,
        [scriptblock]$Block
    )
    Add-Section $Title
    try {
        $text = (& $Block 2>&1 | Out-String -Width 260).TrimEnd()
        if ([string]::IsNullOrWhiteSpace($text)) {
            Add-Line "<no output>"
        } else {
            Add-Line $text
        }
    } catch {
        Add-Line ("ERROR: {0}" -f $_.Exception.Message)
    }
}

function Test-IsAdmin {
    try {
        $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
        $principal = New-Object Security.Principal.WindowsPrincipal($identity)
        return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    } catch {
        return $false
    }
}

function Add-UniqueCandidate {
    param(
        [System.Collections.Generic.List[string]]$List,
        [string]$Path
    )
    if ([string]::IsNullOrWhiteSpace($Path)) {
        return
    }
    $expanded = [Environment]::ExpandEnvironmentVariables($Path.Trim('"'))
    if ([string]::IsNullOrWhiteSpace($expanded)) {
        return
    }
    if (-not $List.Contains($expanded)) {
        [void]$List.Add($expanded)
    }
}

function Add-DriverDirVariants {
    param(
        [System.Collections.Generic.List[string]]$List,
        [string]$Root
    )
    if ([string]::IsNullOrWhiteSpace($Root)) {
        return
    }
    $clean = [Environment]::ExpandEnvironmentVariables($Root.Trim().Trim('"'))
    if ([string]::IsNullOrWhiteSpace($clean)) {
        return
    }
    Add-UniqueCandidate $List $clean
    Add-UniqueCandidate $List (Join-Path $clean "ddhid-driver")
    Add-UniqueCandidate $List (Join-Path $clean "resources")
    Add-UniqueCandidate $List (Join-Path $clean "resources\ddhid-driver")
    Add-UniqueCandidate $List (Join-Path $clean "2.hid\drv")
}

function Get-PathFromCommandText {
    param([string]$Text)
    if ([string]::IsNullOrWhiteSpace($Text)) {
        return ""
    }
    $trimmed = [Environment]::ExpandEnvironmentVariables($Text.Trim())
    if ($trimmed.StartsWith('"')) {
        $end = $trimmed.IndexOf('"', 1)
        if ($end -gt 1) {
            return $trimmed.Substring(1, $end - 1)
        }
    }
    $exeIndex = $trimmed.ToLowerInvariant().IndexOf(".exe")
    if ($exeIndex -gt 0) {
        return $trimmed.Substring(0, $exeIndex + 4).Trim('"')
    }
    return $trimmed.Trim('"')
}

function Add-RegistryInstallCandidates {
    param([System.Collections.Generic.List[string]]$List)
    $roots = @(
        "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall",
        "HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall",
        "HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall"
    )
    foreach ($root in $roots) {
        if (-not (Test-Path -LiteralPath $root)) {
            continue
        }
        try {
            Get-ChildItem -LiteralPath $root -ErrorAction SilentlyContinue | ForEach-Object {
                $p = Get-ItemProperty -LiteralPath $_.PSPath -ErrorAction SilentlyContinue
                $text = @($p.DisplayName, $p.DisplayIcon, $p.InstallLocation, $p.UninstallString, $p.QuietUninstallString) -join " "
                if ($text -notmatch "FlairBloom|flair-bloom|fun\.xwink\.flairbloom") {
                    return
                }
                Add-DriverDirVariants $List $p.InstallLocation
                foreach ($cmd in @($p.DisplayIcon, $p.UninstallString, $p.QuietUninstallString)) {
                    $exe = Get-PathFromCommandText $cmd
                    if (-not [string]::IsNullOrWhiteSpace($exe)) {
                        Add-DriverDirVariants $List (Split-Path -Parent $exe)
                    }
                }
            }
        } catch {
            # Registry probing is best-effort.
        }
    }
}

function Resolve-DriverDir {
    param([string]$InputDir)
    $candidates = New-Object System.Collections.Generic.List[string]

    if (-not [string]::IsNullOrWhiteSpace($InputDir)) {
        Add-DriverDirVariants $candidates $InputDir
    }

    Add-RegistryInstallCandidates $candidates

    Add-DriverDirVariants $candidates (Join-Path $env:LOCALAPPDATA "Programs\FlairBloom")
    Add-DriverDirVariants $candidates (Join-Path $env:LOCALAPPDATA "FlairBloom")
    Add-DriverDirVariants $candidates (Join-Path $env:ProgramFiles "FlairBloom")
    if (${env:ProgramFiles(x86)}) {
        Add-DriverDirVariants $candidates (Join-Path ${env:ProgramFiles(x86)} "FlairBloom")
    }
    Add-DriverDirVariants $candidates "C:\Program Files\FlairBloom"
    Add-DriverDirVariants $candidates "C:\Program Files (x86)\FlairBloom"

    if ($PSScriptRoot) {
        Add-DriverDirVariants $candidates $PSScriptRoot
    }
    Add-DriverDirVariants $candidates (Get-Location).Path

    foreach ($candidate in $candidates) {
        if ((Test-Path -LiteralPath (Join-Path $candidate "ddc.exe")) -or
            (Test-Path -LiteralPath (Join-Path $candidate "ddhid63340.inf"))) {
            return (Resolve-Path -LiteralPath $candidate -ErrorAction SilentlyContinue).Path
        }
    }
    return ""
}

function Add-FileProbe {
    param(
        [string]$Path,
        [string]$ExpectedSha256 = ""
    )
    if ([string]::IsNullOrWhiteSpace($Path)) {
        return
    }

    Add-Line ("-- {0}" -f $Path)
    if (-not (Test-Path -LiteralPath $Path)) {
        Add-Line "exists: false"
        return
    }

    try {
        $item = Get-Item -LiteralPath $Path -Force
        Add-Line ("exists: true")
        Add-Line ("length: {0}" -f $item.Length)
        Add-Line ("last_write_time: {0:o}" -f $item.LastWriteTime)
        Add-Line ("attributes: {0}" -f $item.Attributes)
    } catch {
        Add-Line ("item_error: {0}" -f $_.Exception.Message)
    }

    try {
        $hash = Get-FileHash -LiteralPath $Path -Algorithm SHA256
        Add-Line ("sha256: {0}" -f $hash.Hash)
        if (-not [string]::IsNullOrWhiteSpace($ExpectedSha256)) {
            Add-Line ("sha256_expected: {0}" -f $ExpectedSha256)
            Add-Line ("sha256_match: {0}" -f ($hash.Hash.ToUpperInvariant() -eq $ExpectedSha256.ToUpperInvariant()))
        }
    } catch {
        Add-Line ("hash_error: {0}" -f $_.Exception.Message)
    }

    try {
        $sig = Get-AuthenticodeSignature -LiteralPath $Path
        Add-Line ("signature_status: {0}" -f $sig.Status)
        Add-Line ("signature_status_message: {0}" -f $sig.StatusMessage)
        if ($sig.SignerCertificate) {
            Add-Line ("signer_subject: {0}" -f $sig.SignerCertificate.Subject)
            Add-Line ("signer_thumbprint: {0}" -f $sig.SignerCertificate.Thumbprint)
            Add-Line ("signer_not_before: {0:o}" -f $sig.SignerCertificate.NotBefore)
            Add-Line ("signer_not_after: {0:o}" -f $sig.SignerCertificate.NotAfter)
        }
    } catch {
        Add-Line ("signature_error: {0}" -f $_.Exception.Message)
    }
}

function Add-RegistryKey {
    param([string]$Path)
    Add-Line ("-- {0}" -f $Path)
    if (-not (Test-Path -LiteralPath $Path)) {
        Add-Line "exists: false"
        return
    }
    Add-Line "exists: true"
    try {
        Get-ItemProperty -LiteralPath $Path | Format-List * | Out-String -Width 260 | ForEach-Object { Add-Line $_.TrimEnd() }
    } catch {
        Add-Line ("registry_error: {0}" -f $_.Exception.Message)
    }
}

function Add-SetupApiMatches {
    param([string]$Path)
    Add-Line ("-- {0}" -f $Path)
    if (-not (Test-Path -LiteralPath $Path)) {
        Add-Line "exists: false"
        return
    }

    try {
        $matches = @(Select-String -LiteralPath $Path -Pattern "ddhid63340", "ddxoft", "ddhid63340\\HID_DEVICE" -Context 10, 18 -ErrorAction SilentlyContinue)
        if ($matches.Count -eq 0) {
            Add-Line "No direct DDHID matches found."
            Add-Line "Last 80 setupapi error-ish lines:"
            $errors = @(Select-String -LiteralPath $Path -Pattern "!!!", "#E", "0xe000", "failed", "error" -SimpleMatch -ErrorAction SilentlyContinue | Select-Object -Last 80)
            if ($errors.Count -eq 0) {
                Add-Line "<none>"
            } else {
                $errors | ForEach-Object { Add-Line ("{0}:{1}: {2}" -f $_.Path, $_.LineNumber, $_.Line.TrimEnd()) }
            }
            return
        }

        $matches | Select-Object -Last 12 | ForEach-Object {
            Add-Line ""
            Add-Line ("match_line: {0}" -f $_.LineNumber)
            if ($_.Context.PreContext) {
                $_.Context.PreContext | ForEach-Object { Add-Line ("  {0}" -f $_) }
            }
            Add-Line ("> {0}" -f $_.Line)
            if ($_.Context.PostContext) {
                $_.Context.PostContext | ForEach-Object { Add-Line ("  {0}" -f $_) }
            }
        }
    } catch {
        Add-Line ("setupapi_error: {0}" -f $_.Exception.Message)
    }
}

function Add-EventMatches {
    param(
        [string]$LogName,
        [string]$Pattern,
        [int]$MaxEvents = 300
    )
    Add-Line ("-- {0}" -f $LogName)
    try {
        $events = @(Get-WinEvent -LogName $LogName -MaxEvents $MaxEvents -ErrorAction Stop |
            Where-Object { $_.Message -match $Pattern } |
            Select-Object -First 30)
        if ($events.Count -eq 0) {
            Add-Line "<no matching events>"
            return
        }
        foreach ($event in $events) {
            Add-Line ("time: {0:o}" -f $event.TimeCreated)
            Add-Line ("provider: {0}" -f $event.ProviderName)
            Add-Line ("id: {0}" -f $event.Id)
            Add-Line ("level: {0}" -f $event.LevelDisplayName)
            Add-Line ($event.Message -replace "`r?`n", " ")
            Add-Line ""
        }
    } catch {
        Add-Line ("event_log_error: {0}" -f $_.Exception.Message)
    }
}

function Get-PendingRenameEntries {
    try {
        $value = (Get-ItemProperty -LiteralPath "HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager" -Name "PendingFileRenameOperations" -ErrorAction SilentlyContinue).PendingFileRenameOperations
        if ($null -eq $value) { return @() }
        return @($value)
    } catch {
        return @()
    }
}

$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
if ([string]::IsNullOrWhiteSpace($OutFile)) {
    $desktop = [Environment]::GetFolderPath("Desktop")
    if ([string]::IsNullOrWhiteSpace($desktop)) {
        $desktop = $env:TEMP
    }
    $OutFile = Join-Path $desktop ("ddhid-diagnose-{0}.txt" -f $stamp)
}

$resolvedDriverDir = Resolve-DriverDir -InputDir $DriverDir
$systemDriver = Join-Path $env:SystemRoot "System32\drivers\ddhid63340.sys"
$infDir = Join-Path $env:SystemRoot "INF"
$driverStore = Join-Path $env:SystemRoot "System32\DriverStore\FileRepository"
$appLogDir = Join-Path $env:LOCALAPPDATA "fun.xwink.flairbloom\logs"

$expectedHashes = @{
    "ddc.exe" = "3C535B334F0897B8A0870BCB476C30EA79AFD09CFC18F8E00190BDC7C6C46785"
    "ddhid63340.inf" = "17FE3814F57E98DD2AF97F56B63502E474EA5E41CDA1A510FFE435EE6AD7A104"
    "ddhid63340.cat" = "6135C664711127A62E0988F6844521E345D78ACE9D3747A392400CE99BE96983"
    "ddhid63340.sys" = "FBE510402B3822C63E94752051B7D5895B67875F22EC48593DE19764A649F8B1"
    "ddhid.63340.dll" = "01E8DB6893CF79E9E7AA3AFBEE76BEA6C4220C4D1A2C63BC2E5B7C109FDB831E"
}

Add-Line "DDHID diagnostic report"
Add-Line ("generated_at: {0:o}" -f (Get-Date))
Add-Line ("script_path: {0}" -f $PSCommandPath)
Add-Line ("output_file: {0}" -f $OutFile)
Add-Line ("is_admin: {0}" -f (Test-IsAdmin))
Add-Line ("driver_dir_input: {0}" -f $DriverDir)
Add-Line ("driver_dir_resolved: {0}" -f $resolvedDriverDir)
Add-Line ("evidence_only: {0}" -f $EvidenceOnly.IsPresent)

Add-CommandOutput "OS and PowerShell" {
    $computer = Get-ComputerInfo -Property OsName, OsVersion, OsBuildNumber, OsArchitecture, WindowsProductName -ErrorAction SilentlyContinue
    [PSCustomObject]@{
        ComputerName = $env:COMPUTERNAME
        UserName = $env:USERNAME
        Is64BitOS = [Environment]::Is64BitOperatingSystem
        ProcessorArchitecture = $env:PROCESSOR_ARCHITECTURE
        PowerShellVersion = $PSVersionTable.PSVersion.ToString()
        OsName = $computer.OsName
        OsVersion = $computer.OsVersion
        OsBuildNumber = $computer.OsBuildNumber
        OsArchitecture = $computer.OsArchitecture
        WindowsProductName = $computer.WindowsProductName
    } | Format-List *
}

Add-CommandOutput "Security policy: HVCI / SAC / DeviceGuard" {
    "HVCI registry:"
    Get-ItemProperty -LiteralPath "HKLM:\SYSTEM\CurrentControlSet\Control\DeviceGuard\Scenarios\HypervisorEnforcedCodeIntegrity" -ErrorAction SilentlyContinue | Format-List *
    ""
    "Smart App Control registry:"
    Get-ItemProperty -LiteralPath "HKLM:\SYSTEM\CurrentControlSet\Control\CI\Policy" -ErrorAction SilentlyContinue | Format-List *
    ""
    "Win32_DeviceGuard:"
    Get-CimInstance -ClassName Win32_DeviceGuard -Namespace root\Microsoft\Windows\DeviceGuard -ErrorAction SilentlyContinue | Format-List *
}

Add-Section "Pending reboot markers"
$pendingRename = @(Get-PendingRenameEntries)
Add-Line ("PendingFileRenameOperations_count: {0}" -f $pendingRename.Count)
if ($pendingRename.Count -gt 0) {
    Add-Line "PendingFileRenameOperations_first_80:"
    $pendingRename | Select-Object -First 80 | ForEach-Object { Add-Line ("  {0}" -f $_) }
}
Add-Line ("CBS_RebootPending: {0}" -f (Test-Path -LiteralPath "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Component Based Servicing\RebootPending"))
Add-Line ("WindowsUpdate_RebootRequired: {0}" -f (Test-Path -LiteralPath "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\WindowsUpdate\Auto Update\RebootRequired"))

if (-not $EvidenceOnly) {
    Add-Section "DDHID package files"
    if ([string]::IsNullOrWhiteSpace($resolvedDriverDir)) {
        Add-Line "No DriverDir was resolved. Re-run with -DriverDir pointing to the folder that contains ddc.exe."
    } else {
        Add-Line ("DriverDir: {0}" -f $resolvedDriverDir)
        Add-CommandOutput "DriverDir listing" {
            Get-ChildItem -LiteralPath $resolvedDriverDir -Force | Sort-Object Name | Format-Table Mode, LastWriteTime, Length, Name -AutoSize
        }
        foreach ($name in @("ddc.exe", "ddhid63340.inf", "ddhid63340.cat", "ddhid63340.sys")) {
            Add-FileProbe -Path (Join-Path $resolvedDriverDir $name) -ExpectedSha256 $expectedHashes[$name]
        }

        $parentDll = Join-Path (Split-Path -Parent $resolvedDriverDir) "ddhid.63340.dll"
        $sameDirDll = Join-Path $resolvedDriverDir "ddhid.63340.dll"
        if (Test-Path -LiteralPath $parentDll) {
            Add-FileProbe -Path $parentDll -ExpectedSha256 $expectedHashes["ddhid.63340.dll"]
        } elseif (Test-Path -LiteralPath $sameDirDll) {
            Add-FileProbe -Path $sameDirDll -ExpectedSha256 $expectedHashes["ddhid.63340.dll"]
        } else {
            Add-Line "-- ddhid.63340.dll"
            Add-Line "not found next to driver folder or in driver folder"
        }
    }
}

Add-Section "Installed DDHID state"
Add-FileProbe -Path $systemDriver
Add-RegistryKey -Path "HKLM:\SYSTEM\CurrentControlSet\Services\ddhid63340"
Add-CommandOutput "sc.exe ddhid63340" {
    sc.exe query ddhid63340
    ""
    sc.exe qc ddhid63340
}
Add-CommandOutput "ControlSet service keys" {
    Get-ChildItem -LiteralPath "HKLM:\SYSTEM" -ErrorAction SilentlyContinue |
        Where-Object { $_.PSChildName -like "ControlSet*" } |
        ForEach-Object {
            $path = "Registry::{0}\Services\ddhid63340" -f $_.Name
            if (Test-Path -LiteralPath $path) {
                "FOUND: $path"
                Get-ItemProperty -LiteralPath $path | Format-List Type, Start, ErrorControl, ImagePath, DisplayName, DriverDelete, DeleteFlag
            }
        }
}

Add-CommandOutput "Windows INF entries containing ddhid63340" {
    $matches = @(Select-String -Path (Join-Path $infDir "oem*.inf") -Pattern "ddhid63340", "www.ddxoft.com" -SimpleMatch -ErrorAction SilentlyContinue)
    if ($matches.Count -eq 0) {
        "No matching OEM INF files."
    } else {
        $matches | Select-Object Path, LineNumber, Line | Format-Table -Wrap -AutoSize
        ""
        "Unique INF paths:"
        $matches | Select-Object -ExpandProperty Path -Unique
    }
}

Add-CommandOutput "DriverStore ddhid directories" {
    Get-ChildItem -LiteralPath $driverStore -Filter "ddhid*" -Force -ErrorAction SilentlyContinue | Format-Table Mode, LastWriteTime, Length, Name -AutoSize
}

Add-CommandOutput "PnP driver/device view" {
    "pnputil /enum-drivers /class HIDClass"
    pnputil.exe /enum-drivers /class HIDClass
    ""
    "pnputil /enum-devices /class HIDClass /drivers"
    pnputil.exe /enum-devices /class HIDClass /drivers
    ""
    "Get-PnpDevice DDHID filtered"
    Get-PnpDevice -Class HIDClass -ErrorAction SilentlyContinue |
        Where-Object {
            ($_.InstanceId -match "ddhid|63340") -or
            ($_.FriendlyName -match "ddhid|63340") -or
            ($_.Manufacturer -match "ddxoft|ddhid|63340") -or
            ($_.Service -match "ddhid|63340")
        } |
        Format-List *
}

Add-Section "SetupAPI log snippets"
Add-SetupApiMatches -Path (Join-Path $infDir "setupapi.dev.log")
Add-SetupApiMatches -Path (Join-Path $infDir "setupapi.app.log")

Add-Section "Relevant event logs"
Add-EventMatches -LogName "System" -Pattern "ddhid|63340|ddc\.exe" -MaxEvents 500
Add-EventMatches -LogName "Microsoft-Windows-CodeIntegrity/Operational" -Pattern "ddhid|63340|ddc\.exe|flair|block|signature|policy" -MaxEvents 500
Add-EventMatches -LogName "Microsoft-Windows-Windows Defender/Operational" -Pattern "ddhid|63340|ddc\.exe|flair|quarantine|blocked|threat" -MaxEvents 500

Add-Section "FlairBloom app logs"
Add-Line ("app_log_dir: {0}" -f $appLogDir)
if (-not (Test-Path -LiteralPath $appLogDir)) {
    Add-Line "app_log_dir_exists: false"
} else {
    Add-Line "app_log_dir_exists: true"
    Add-CommandOutput "Latest app log files" {
        Get-ChildItem -LiteralPath $appLogDir -Force |
            Sort-Object LastWriteTime -Descending |
            Select-Object -First 8 Mode, LastWriteTime, Length, Name |
            Format-Table -AutoSize
    }
    Add-CommandOutput "App log DDHID/error lines" {
        $files = @(Get-ChildItem -LiteralPath $appLogDir -File -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending | Select-Object -First 5)
        foreach ($file in $files) {
            "-- $($file.FullName)"
            Select-String -LiteralPath $file.FullName -Pattern "DD-HID", "ddhid", "ddc.exe", "driver", "驱动", "error", "failed", "失败" -SimpleMatch -ErrorAction SilentlyContinue |
                Select-Object -Last 80 |
                ForEach-Object { "{0}:{1}: {2}" -f $_.Path, $_.LineNumber, $_.Line }
        }
    }
}

if (-not $EvidenceOnly) {
    Add-Section "Computed hints"
    $sysExists = Test-Path -LiteralPath $systemDriver
    $svcExists = Test-Path -LiteralPath "HKLM:\SYSTEM\CurrentControlSet\Services\ddhid63340"
    $infMatches = @(Select-String -Path (Join-Path $infDir "oem*.inf") -Pattern "ddhid63340", "www.ddxoft.com" -SimpleMatch -ErrorAction SilentlyContinue)
    $storeMatches = @(Get-ChildItem -LiteralPath $driverStore -Filter "ddhid*" -Force -ErrorAction SilentlyContinue)
    $pendingAny = ($pendingRename.Count -gt 0) -or
        (Test-Path -LiteralPath "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Component Based Servicing\RebootPending") -or
        (Test-Path -LiteralPath "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\WindowsUpdate\Auto Update\RebootRequired")

    Add-Line ("sys_exists: {0}" -f $sysExists)
    Add-Line ("service_exists: {0}" -f $svcExists)
    Add-Line ("matching_oem_inf_count: {0}" -f $infMatches.Count)
    Add-Line ("driverstore_ddhid_dir_count: {0}" -f $storeMatches.Count)
    Add-Line ("pending_reboot_marker_exists: {0}" -f $pendingAny)

    if ($sysExists -and $svcExists) {
        Add-Line "hint: DDHID looks installed at file+service level. If the app still fails, check DD_btn(0), admin privilege, app log, and CodeIntegrity events."
    } elseif (-not $sysExists -and $svcExists) {
        Add-Line "hint: Service key exists but ddhid63340.sys is missing. This is a likely half-uninstalled/pending-reboot state; reboot first, then repair residue if it remains."
    } elseif ($sysExists -and -not $svcExists) {
        Add-Line "hint: ddhid63340.sys exists but service key is missing. This is an inconsistent install state; reboot and clean/reinstall via PnP."
    } else {
        Add-Line "hint: DDHID is not installed at file+service level. If install fails, SetupAPI snippets and CodeIntegrity/Defender events are the key evidence."
    }
}

try {
    $outDir = Split-Path -Parent $OutFile
    if (-not [string]::IsNullOrWhiteSpace($outDir)) {
        New-Item -ItemType Directory -Path $outDir -Force | Out-Null
    }
    $script:Lines | Set-Content -LiteralPath $OutFile -Encoding UTF8
    Write-Host "DDHID diagnostic report written to:"
    Write-Host $OutFile
} catch {
    Write-Error ("Failed to write report: {0}" -f $_.Exception.Message)
    exit 1
}
