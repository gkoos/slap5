# Plays high5 against another engine with Piskvork's batch mode: each opening is played twice,
# once with each engine moving first. Results are written to <OutDir>/<opponent>_<time>/.
# Any running process named pbrain-* is killed after every game (Piskvork leaves engines behind).
param(
    [Parameter(Mandatory)] [string]$Opponent,
    [int]$Pairs = 10,
    [int]$TimeTurnMs = 1000,
    [int]$TimeMatchS = 300,
    [string]$Engine = "$PSScriptRoot\..\target\release\pbrain-high5.exe",
    [string]$Piskvork = "$PSScriptRoot\..\external\piskvork\bin\piskvork.exe",
    [string]$OutDir = "$PSScriptRoot\..\logs"
)
$ErrorActionPreference = 'Stop'

$engine = (Resolve-Path $Engine).Path
$opponent = (Resolve-Path $Opponent).Path
$piskvork = (Resolve-Path $Piskvork).Path
$openings = 41 # lines in Piskvork's openings.txt
$name = [IO.Path]::GetFileNameWithoutExtension($opponent)
New-Item -ItemType Directory -Force $OutDir | Out-Null
$run = Join-Path (Resolve-Path $OutDir).Path ("{0}_{1:yyyyMMdd_HHmmss}" -f $name, (Get-Date))
New-Item -ItemType Directory -Force $run | Out-Null

$rows = foreach ($i in 0..($Pairs - 1)) {
    foreach ($first in 'high5', 'opponent') {
        $a, $b = if ($first -eq 'high5') { $engine, $opponent } else { $opponent, $engine }
        $psq = Join-Path $run ("p{0:D2}_{1}_first.psq" -f $i, $first)
        $pipeDir = ($run -replace '\\', '/') + '/'
        $arguments = @('-p', "`"$a`"", "`"$b`"", '-rule', '0', '-timeturn', $TimeTurnMs, '-timematch', $TimeMatchS,
            '-opening', ($i % $openings), '-outfile', "`"$psq`"", '-logpipe', "`"$pipeDir`"")
        $p = Start-Process $piskvork -ArgumentList $arguments -WorkingDirectory (Split-Path $piskvork) -PassThru
        $code = -1
        if ($p.WaitForExit(($TimeMatchS * 2 + 120) * 1000)) { $code = $p.ExitCode } else { $p.Kill() }
        Get-Process -Name 'pbrain-*' -ErrorAction SilentlyContinue | Stop-Process -Force

        # Exit codes: 1 first player won, 2 second player won, 3 draw. No game record means the game never ran.
        $outcome = if (-not (Test-Path $psq) -or $code -notin 1, 2, 3) { 'error' }
        elseif ($code -eq 3) { 'draw' }
        elseif (($code -eq 1) -eq ($first -eq 'high5')) { 'win' }
        else { 'loss' }
        $row = [pscustomobject]@{ Pair = $i; Opening = $i % $openings; High5First = ($first -eq 'high5'); ExitCode = $code; Outcome = $outcome }
        Write-Host ("pair {0} high5 {1}: {2}" -f $i, $(if ($row.High5First) { 'first ' } else { 'second' }), $outcome)
        $row
    }
}

$rows | Export-Csv -NoTypeInformation (Join-Path $run 'results.csv')
$tally = { param($rs) '{0}W {1}L {2}D {3}E' -f @($rs | ? Outcome -eq 'win').Count, @($rs | ? Outcome -eq 'loss').Count, @($rs | ? Outcome -eq 'draw').Count, @($rs | ? Outcome -eq 'error').Count }
Write-Host "high5 vs ${name}: $(& $tally $rows)  (as first: $(& $tally ($rows | ? High5First)); as second: $(& $tally ($rows | ? { -not $_.High5First })))"
Write-Host "results: $run"
