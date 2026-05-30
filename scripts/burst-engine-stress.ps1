param(
  [string]$ProcessName = "FlairBloom",
  [int]$DurationSeconds = 60,
  [int]$SampleIntervalMs = 500,
  [string]$OutputPath = "target/burst-engine-stress.csv"
)

$ErrorActionPreference = "Stop"

$outDir = Split-Path -Parent $OutputPath
if ($outDir -and !(Test-Path $outDir)) {
  New-Item -ItemType Directory -Force -Path $outDir | Out-Null
}

$process = Get-Process -Name $ProcessName -ErrorAction Stop | Select-Object -First 1
$logicalCores = [Environment]::ProcessorCount
$previousCpu = $process.CPU
$previousTime = Get-Date
$deadline = $previousTime.AddSeconds($DurationSeconds)

"timestamp,pid,cpu_percent,threads,working_set_mb,private_memory_mb,handles" | Set-Content -Encoding UTF8 $OutputPath

while ((Get-Date) -lt $deadline) {
  Start-Sleep -Milliseconds $SampleIntervalMs
  $now = Get-Date
  $process.Refresh()

  $cpuDelta = [Math]::Max(0, $process.CPU - $previousCpu)
  $timeDelta = [Math]::Max(0.001, ($now - $previousTime).TotalSeconds)
  $cpuPercent = ($cpuDelta / $timeDelta / $logicalCores) * 100

  $row = "{0},{1},{2:N2},{3},{4:N2},{5:N2},{6}" -f `
    $now.ToString("o"), `
    $process.Id, `
    $cpuPercent, `
    $process.Threads.Count, `
    ($process.WorkingSet64 / 1MB), `
    ($process.PrivateMemorySize64 / 1MB), `
    $process.HandleCount
  Add-Content -Encoding UTF8 $OutputPath $row

  $previousCpu = $process.CPU
  $previousTime = $now
}

Write-Host "Burst engine stress samples written to $OutputPath"
