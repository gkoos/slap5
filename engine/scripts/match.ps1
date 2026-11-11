# Plays slap5 against another engine with Piskvork's batch mode: each opening is played twice,
# once with each engine moving first. Results are written to <OutDir>/<opponent>_<time>/.
# Any running process named pbrain-* is killed after every game (Piskvork leaves engines behind).
# A game that outlives the match clock by 90s is killed and recorded as `stuck` rather than blocking the
# run: Piskvork occasionally wedges on an opponent that has exited. Each run also writes slap5-trace.log,
# a timestamped record of what the engine read and wrote, so which side froze is checkable afterwards.
param(
    [Parameter(Mandatory)] [string]$Opponent,
    [int]$Pairs = 10,
    [int]$TimeTurnMs = 1000,
    [int]$TimeMatchS = 600,
    [int]$OpeningOffset = 0,
    [string]$Engine = "",
    [string]$Piskvork = "",
    [string]$Openings = "",
    [string]$OutDir = "",
    # Search switches for the engine, space separated ("root-history clustered-tt"; see `Config::apply`).
    # Passed through the environment, which is how Piskvork starts the engines, so an A/B of switches needs
    # no rebuild and the run records which settings were in force.
    [string]$Flags = ""
)
$ErrorActionPreference = 'Stop'

# This script lives in engine/scripts, but every default below is relative to the repository root: the
# workspace's target directory, the third-party engines and the match logs all stay at that level. The
# root is resolved here rather than in the parameter block because $PSScriptRoot is empty there when the
# script is started with `powershell -File`.
$engineDir = if ($PSScriptRoot) { Split-Path $PSScriptRoot } else { (Get-Location).Path }
$repo = Split-Path $engineDir
if (-not $Engine) { $Engine = Join-Path $repo 'target\release\pbrain-slap5.exe' }
if (-not $Piskvork) { $Piskvork = Join-Path $repo 'external\piskvork\bin\piskvork.exe' }
if (-not $OutDir) { $OutDir = Join-Path $repo 'logs' }

$engine = (Resolve-Path $Engine).Path
$opponent = (Resolve-Path $Opponent).Path
$piskvork = (Resolve-Path $Piskvork).Path
$openingsPath = Join-Path (Split-Path $piskvork) 'openings.txt'
# Piskvork reads openings.txt from its own directory and has no option for another file, so a chosen list
# is copied in for the run and the previous one put back at the end. Which list was in force is recorded,
# because the pair indices in run-info.txt only name positions through the list they were played on.
$openingsBackup = "$openingsPath.slap5-backup"
if ($Openings) {
    $Openings = (Resolve-Path $Openings).Path
    if (-not (Test-Path $openingsBackup)) { Copy-Item $openingsPath $openingsBackup }
    Copy-Item $Openings $openingsPath -Force
}
$listPath = if ($Openings) { $Openings } else { $openingsPath }
$openings = @(Get-Content $listPath | Where-Object { $_.Trim() }).Count
if ($openings -lt 1) { throw "no openings found in $listPath" }
$name = [IO.Path]::GetFileNameWithoutExtension($opponent)
New-Item -ItemType Directory -Force $OutDir | Out-Null
$run = Join-Path (Resolve-Path $OutDir).Path ("{0}_{1:yyyyMMdd_HHmmss}" -f $name, (Get-Date))
New-Item -ItemType Directory -Force $run | Out-Null

# A timestamped trace of what the engine read and wrote, for telling a frozen engine from a frozen
# harness. Piskvork passes its environment to the engines it starts, so this reaches slap5 (the
# third-party opponents ignore it). Unset, the engine writes nothing.
$env:SLAP5_TRACE = Join-Path $run 'slap5-trace.log'
# The engine's search switches travel the same way. Cleared rather than set empty, so an inherited value
# from the calling shell cannot quietly become part of a control run.
if ($Flags) { $env:SLAP5_FLAGS = $Flags } else { Remove-Item Env:\SLAP5_FLAGS -ErrorAction SilentlyContinue }

# Record what was run: parameters, engine files, the opening list's identity and the repository revision.
# The openings file's hash is here because the pair indices listed below only identify positions through it:
# if that file changes, opening 25 in an old run is a different position from opening 25 in a new one, and
# per-opening records pooled across the two are meaningless. The engine's search settings are the built-in
# defaults (no command-line switches are passed to it).
$revision = (git -C $repo rev-parse --short HEAD 2>$null)
$dirty = @(git -C $repo status --porcelain 2>$null).Count
@(
    "engine: $engine"
    "engine hash: $((Get-FileHash $engine -Algorithm SHA256).Hash.Substring(0, 16))"
    "opponent: $opponent"
    "openings file: $listPath"
    "openings hash: $((Get-FileHash $listPath -Algorithm SHA256).Hash.Substring(0, 16)); total openings: $openings"
    "pairs: $Pairs; openings: $((0..($Pairs - 1) | ForEach-Object { ($_ + $OpeningOffset) % $openings }) -join ',')"
    "timeturn_ms: $TimeTurnMs; timematch_s: $TimeMatchS; rule: 0 (freestyle); board size: Piskvork registry"
    "engine settings: $(if ($Flags) { $Flags } else { 'defaults (Config::default)' })"
    "revision: $revision; uncommitted files: $dirty"
    "trace: $env:SLAP5_TRACE"
    "started: $(Get-Date -Format s)"
) | Set-Content (Join-Path $run 'run-info.txt')

# Running tally, printed after every game so a long block can be watched while it runs.
$seen = @{ win = 0; loss = 0; draw = 0; truncated = 0; stuck = 0; error = 0 }
$rows = foreach ($i in 0..($Pairs - 1)) {
    foreach ($first in 'slap5', 'opponent') {
        $a, $b = if ($first -eq 'slap5') { $engine, $opponent } else { $opponent, $engine }
        $psq = Join-Path $run ("p{0:D2}_{1}_first.psq" -f $i, $first)
        $pipeDir = ($run -replace '\\', '/') + '/'
        $arguments = @('-p', "`"$a`"", "`"$b`"", '-rule', '0', '-timeturn', $TimeTurnMs, '-timematch', $TimeMatchS,
            '-opening', (($i + $OpeningOffset) % $openings), '-outfile', "`"$psq`"", '-logpipe', "`"$pipeDir`"")
        $p = Start-Process $piskvork -ArgumentList $arguments -WorkingDirectory (Split-Path $piskvork) -PassThru
        $code = -1
        $stuck = $false
        # The match clock is per engine, so a legitimate game can spend nearly twice TimeMatchS in wall time;
        # anything past both allowances plus a margin is a hang - usually Piskvork waiting on an engine that
        # has already exited. Kill it and record it, instead of blocking for the better part of an hour on a
        # game that will never finish.
        $allowance = $TimeMatchS * 2 + 90
        if ($p.WaitForExit($allowance * 1000)) {
            $code = $p.ExitCode
        } else {
            $stuck = $true
            $p.Kill()
            Write-Host ("  stuck: killed after {0}s" -f $allowance)
        }
        Get-Process -Name 'pbrain-*' -ErrorAction SilentlyContinue | Stop-Process -Force

        # Exit codes: 1 first player won, 2 second player won, 3 draw or stopped. A finished draw needs a
        # full board (225 moves); a code 3 with a partial record is the match clock stopping a long game,
        # which the tally reports as truncated rather than as an error. A stuck game never returned an
        # exit code at all, so it is its own outcome.
        $moves = if (Test-Path $psq) { @(Get-Content $psq | Where-Object { $_ -match '^\d+,\d+,\d+$' }).Count } else { 0 }
        $outcome = if ($stuck) { 'stuck' }
        elseif ($moves -eq 0 -or $code -notin 1, 2, 3) { 'error' }
        elseif ($code -eq 3) { if ($moves -eq 225) { 'draw' } else { 'truncated' } }
        elseif (($code -eq 1) -eq ($first -eq 'slap5')) { 'win' }
        else { 'loss' }
        $row = [pscustomobject]@{ Pair = $i; Opening = (($i + $OpeningOffset) % $openings); Slap5First = ($first -eq 'slap5'); ExitCode = $code; Moves = $moves; Outcome = $outcome }
        Write-Host ("pair {0} slap5 {1}: {2}" -f $i, $(if ($row.Slap5First) { 'first ' } else { 'second' }), $outcome)
        $seen[$outcome] += 1
        $played = ($seen.Values | Measure-Object -Sum).Sum
        Write-Host ("  after {0,2} games: {1,2}W {2,2}L {3}D {4}T {5}S {6}E" -f $played, $seen.win, $seen.loss, $seen.draw, $seen.truncated, $seen.stuck, $seen.error)
        $row
    }
}

# The chosen list goes back out of Piskvork's directory here; the backup stays behind only if this script
# was interrupted, and then it is the file to copy back by hand.
if ($Openings -and (Test-Path $openingsBackup)) { Move-Item -Force $openingsBackup $openingsPath }

$rows | Export-Csv -NoTypeInformation (Join-Path $run 'results.csv')
$tally = { param($rs) '{0}W {1}L {2}D {3}T {4}S {5}E' -f @($rs | ? Outcome -eq 'win').Count, @($rs | ? Outcome -eq 'loss').Count, @($rs | ? Outcome -eq 'draw').Count, @($rs | ? Outcome -eq 'truncated').Count, @($rs | ? Outcome -eq 'stuck').Count, @($rs | ? Outcome -eq 'error').Count }
Write-Host "slap5 vs ${name}: $(& $tally $rows)  (as first: $(& $tally ($rows | ? Slap5First)); as second: $(& $tally ($rows | ? { -not $_.Slap5First })))"
Write-Host "results: $run"
