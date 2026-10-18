param([string]$Directory = "$PSScriptRoot/../test-results/native-release")
$ErrorActionPreference = 'Stop'
$progressPath = Join-Path $Directory 'progress.json'
$deadline = (Get-Date).AddMinutes(20)
while (!(Test-Path -LiteralPath $progressPath)) {
    if ((Get-Date) -gt $deadline) { throw 'Native acceptance run did not start.' }
    Start-Sleep -Seconds 2
}
$rows = [System.Collections.Generic.List[object]]::new()
$started = Get-Date
while ((Get-Date) -lt $deadline) {
    try { $state = Get-Content -LiteralPath $progressPath -Raw | ConvertFrom-Json } catch { Start-Sleep -Milliseconds 200; continue }
    $rootProcess = Get-Process -Id $state.pid -ErrorAction SilentlyContinue
    if (!$rootProcess) { break }
    $processes = @(Get-CimInstance Win32_Process -Filter "Name = 'my-snip.exe' OR Name = 'msedgewebview2.exe'")
    $ids = [System.Collections.Generic.HashSet[int]]::new()
    [void]$ids.Add([int]$state.pid)
    do {
        $added = $false
        foreach ($proc in $processes) {
            if ($ids.Contains([int]$proc.ParentProcessId) -and $ids.Add([int]$proc.ProcessId)) { $added = $true }
        }
    } while ($added)
    $owned = @(Get-Process -Id @($ids) -ErrorAction SilentlyContinue)
    $row = [pscustomobject]@{
        timestamp = (Get-Date).ToUniversalTime().ToString('o')
        elapsedSeconds = [math]::Round(((Get-Date) - $started).TotalSeconds, 1)
        stage = $state.stage
        processCount = $owned.Count
        webviewProcessCount = @($owned | Where-Object ProcessName -eq 'msedgewebview2').Count
        workingSetMiB = [math]::Round(($owned | Measure-Object WorkingSet64 -Sum).Sum / 1MB, 2)
        privateMiB = [math]::Round(($owned | Measure-Object PrivateMemorySize64 -Sum).Sum / 1MB, 2)
        rootWorkingSetMiB = [math]::Round($rootProcess.WorkingSet64 / 1MB, 2)
    }
    $rows.Add($row)
    $row | Export-Csv -LiteralPath (Join-Path $Directory 'memory-samples.csv') -Append -NoTypeInformation -Encoding utf8
    if ($state.stage -in @('passed','failed')) { break }
    Start-Sleep -Seconds 1
}
if (!$rows.Count) { throw 'No process memory samples were captured.' }
$os = Get-CimInstance Win32_OperatingSystem
$computer = Get-CimInstance Win32_ComputerSystem
$summary = [ordered]@{
    sampledBuild = 'release with opt-in diagnostics and bundled frontend; no DevTools'
    os = $os.Caption
    osVersion = $os.Version
    logicalProcessors = $computer.NumberOfLogicalProcessors
    installedRamGiB = [math]::Round($computer.TotalPhysicalMemory / 1GB, 1)
    methodology = 'Sum of app and descendant WebView2 processes. Working sets can double-count shared pages. Private MiB is private committed memory. Sample interval approximately 1 second; peak is sampled, not absolute.'
    samples = $rows.Count
    sampledPeakWorkingSetMiB = ($rows | Measure-Object workingSetMiB -Maximum).Maximum
    sampledPeakPrivateMiB = ($rows | Measure-Object privateMiB -Maximum).Maximum
    resultVisible = @($rows | Where-Object stage -eq 'result-visible')
    idle30Seconds = @($rows | Where-Object stage -eq 'idle-30-seconds' | Select-Object -First 1)
    idle120Seconds = @($rows | Where-Object stage -eq 'idle-120-seconds' | Select-Object -First 1)
}
$summary | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $Directory 'memory-summary.json') -Encoding utf8
$summary | ConvertTo-Json -Depth 6
