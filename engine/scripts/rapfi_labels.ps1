# Queries Rapfi's evaluation for each position in an eval_dump, one fresh process per position: the
# protocol has no "clear hash" and Rapfi exits after a BOARD. The label is the Eval at the first reported
# depth >= Depth rather than the last one, so it does not move with machine load.
# Usage: powershell -NoProfile -File engine\scripts\rapfi_labels.ps1 -Dump <file> -Out <file> [-Depth 8]
param(
    [Parameter(Mandatory)] [string]$Dump,
    [Parameter(Mandatory)] [string]$Out,
    [int]$Depth = 8,
    [int]$TimeTurnMs = 300,
    [int]$TimeoutSec = 15,
    [string]$Rapfi = "C:\projects\high5\external\rapfi\bin\pbrain-rapfi_avx2.exe"
)
$ErrorActionPreference = 'Continue'
$rapfi = (Resolve-Path $Rapfi).Path
$rapfiDir = Split-Path $rapfi
$in = Join-Path $env:TEMP 'rapfi_q_in.txt'
$raw = Join-Path $env:TEMP 'rapfi_q_out.txt'

$rows = @(Get-Content $Dump | Where-Object { $_ -match '^(\S+)#(\d+) us=(\d+) static=(-?\d+) stones=' })
$lines = New-Object System.Collections.Generic.List[string]
$lines.Add("# name`tply`tus`tstatic`trapfi_eval`trapfi_depth`tms")
$n = 0
$skipped = 0
$sw = [Diagnostics.Stopwatch]::StartNew()
foreach ($row in $rows) {
    if (-not ($row -match '^(\S+)#(\d+) us=(\d+) static=(-?\d+) stones=(.*)$')) { continue }
    $name = $Matches[1]; $ply = [int]$Matches[2]; $us = [int]$Matches[3]; $static = [int]$Matches[4]
    # The stones run to the first `k=v` token; eval_dump appends its fit features after them.
    $stoneText = ($Matches[5] -split ' \w+=')[0]
    $inp = New-Object System.Collections.Generic.List[string]
    $inp.Add('START 15'); $inp.Add("INFO timeout_turn $TimeTurnMs"); $inp.Add('INFO timeout_match 600000')
    $inp.Add('INFO max_memory 83886080'); $inp.Add('BOARD')
    foreach ($s in $stoneText.Split(' ', [System.StringSplitOptions]::RemoveEmptyEntries)) {
        if ($s -match '^(\d+):(\d+),(\d+)$') {
            # BOARD's owner field is relative to the receiving engine - 1 is its own stone, 2 the opponent's -
            # and the label has to be the *mover's* evaluation, so the side to move gets owner 1. Writing the
            # board's player number instead asked Rapfi to evaluate the wrong side on every `us=1` row.
            $owner = if ([int]$Matches[1] -eq $us) { 1 } else { 2 }
            $inp.Add(('{0},{1},{2}' -f $Matches[2], $Matches[3], $owner))
        }
    }
    $inp.Add('DONE')
    $inp | Set-Content -Encoding ascii $in
    $eval = 'na'; $dep = 'na'
    # A fresh process per query, retried once if it produced no Eval at all.
    for ($attempt = 1; $attempt -le 2 -and $eval -eq 'na'; $attempt++) {
        Remove-Item $raw -ErrorAction SilentlyContinue
        $t = [Diagnostics.Stopwatch]::StartNew()
        $p = Start-Process -FilePath $rapfi -WorkingDirectory $rapfiDir -RedirectStandardInput $in -RedirectStandardOutput $raw -PassThru -NoNewWindow
        if (-not $p.WaitForExit($TimeoutSec * 1000)) { $p.Kill() }
        $t.Stop()
        if (Test-Path $raw) {
            foreach ($l in Get-Content $raw) {
                if ($l -match 'Depth (\d+)-\d+ \| Eval (-?\d+|[+-]M\d+)') {
                    # First line at or past the pinned depth, else the deepest one marked @depth. A mate
                    # comes as `+M13`, kept as-is: the fit drops those rows rather than read a large score.
                    $eval = $Matches[2]; $dep = [int]$Matches[1]
                    if ($dep -ge $Depth) { break }
                }
            }
        }
    }
    if ($eval -eq 'na') { $skipped++ } elseif ($eval -notmatch '^[+-]?M\d+$' -and $dep -lt $Depth) { $eval = ('{0}@{1}' -f $eval, $dep) }
    $lines.Add(("{0}`t{1}`t{2}`t{3}`t{4}`t{5}`t{6}" -f $name, $ply, $us, $static, $eval, $dep, $t.ElapsedMilliseconds))
    $n++
    if ($n % 20 -eq 0) {
        $rate = $n / [Math]::Max(0.001, $sw.Elapsed.TotalSeconds)
        $eta = [Math]::Round((($rows.Count - $n) / [Math]::Max(0.001, $rate)) / 60, 1)
        Write-Host ("  {0}/{1}  {2:N2}/s  eta {3} min  no-label {4}" -f $n, $rows.Count, $rate, $eta, $skipped)
        $lines | Set-Content -Encoding utf8 $Out
    }
}
$lines | Set-Content -Encoding utf8 $Out
Write-Host "wrote $Out  rows=$n  without a depth-$Depth label=$skipped"
