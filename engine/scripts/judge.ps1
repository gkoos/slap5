# Plays slap5 against other engines under GomocupJudge, the referee Gomocup itself runs: a fresh engine
# process per game, START and INFO sent without waiting for OK, the side not to move suspended, the per-move
# limit plus tolerance, and memory measured as USS after every move. Gauntlet mode: slap5 plays every
# opponent on every opening with both colours. Each run is self-contained under <OutDir>/judge_<names>_<time>/:
# the server, its result directory (one .psq per game, _result.txt, BayesElo ratings.txt and los.txt), the
# client working directories, slap5-trace.log and run-info.txt.
#
# Search switches go into slap5-settings.txt inside slap5's zip, not the environment: the client starts
# every engine with its own environment and tells engines apart by checksum, so two arms are two zips.
param(
    [Parameter(Mandatory)] [string[]]$Opponent,
    [string]$Engine = "",
    [string]$Flags = "",
    [string]$Openings = "",
    # Play only the first N openings of the list; 0 plays all of them.
    [int]$Count = 0,
    [int]$TimeTurnMs = 1000,
    [int]$TimeMatchMs = 600000,
    [int]$ToleranceMs = 1000,
    [long]$MemoryBytes = 1073741824,
    # Games played at once, one client each, every client pinned to its own core as Gomocup pins it.
    [int]$Clients = 1,
    [int]$Port = 50007,
    [string]$Judge = "",
    [string]$Python = "python",
    [string]$OutDir = ""
)
$ErrorActionPreference = 'Stop'

$engineDir = if ($PSScriptRoot) { Split-Path $PSScriptRoot } else { (Get-Location).Path }
$repo = Split-Path $engineDir
if (-not $Engine) { $Engine = Join-Path $repo 'target\release\pbrain-slap5.exe' }
if (-not $Openings) { $Openings = Join-Path $engineDir 'openings\gomocup-freestyle15-2026.txt' }
if (-not $Judge) { $Judge = Join-Path (Split-Path $repo) 'GomocupJudge' }
if (-not $OutDir) { $OutDir = Join-Path $repo 'logs' }

$engine = (Resolve-Path $Engine).Path
$Openings = (Resolve-Path $Openings).Path
$Judge = (Resolve-Path $Judge).Path
$opponents = @($Opponent | ForEach-Object { (Resolve-Path $_).Path })
if ($Clients -lt 1 -or $Clients -ge [Environment]::ProcessorCount) {
    throw "-Clients must be between 1 and $([Environment]::ProcessorCount - 1)"
}

$lines = @(Get-Content $Openings | Where-Object { $_.Trim() })
if ($Count -gt 0) { $lines = @($lines | Select-Object -First $Count) }
if ($lines.Count -lt 1) { throw "no openings found in $Openings" }

$names = ($opponents | ForEach-Object { [IO.Path]::GetFileNameWithoutExtension($_) }) -join '+'
New-Item -ItemType Directory -Force $OutDir | Out-Null
$run = Join-Path (Resolve-Path $OutDir).Path ("judge_{0}_{1:yyyyMMdd_HHmmss}" -f $names, (Get-Date))
$server = Join-Path $run 'server'
foreach ($d in 'engine', 'opening', 'tournament') { New-Item -ItemType Directory -Force (Join-Path $server $d) | Out-Null }
# server.py reads its configuration, engines and openings relative to its own location, so it runs from a copy.
Copy-Item (Join-Path $Judge 'server\server.py'), (Join-Path $Judge 'server\bayeselo.exe') $server

$trace = Join-Path $run 'slap5-trace.log'
$stage = Join-Path $run 'slap5-package'
New-Item -ItemType Directory -Force $stage | Out-Null
Copy-Item $engine $stage
# UTF-8 without a BOM, as Rust reads it; the trace path is on its own line because it may contain spaces.
[IO.File]::WriteAllLines((Join-Path $stage 'slap5-settings.txt'), @(@($Flags.Trim(), "trace=$trace") | Where-Object { $_ }), (New-Object Text.UTF8Encoding $false))
$zip = Join-Path $server 'engine\slap5.zip'
Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $zip
$engineFiles = @('slap5.zip')
foreach ($o in $opponents) {
    Copy-Item $o (Join-Path $server 'engine')
    $engineFiles += Split-Path $o -Leaf
}

$lines | Set-Content -Encoding ASCII (Join-Path $server 'opening\openings.txt')
$tournament = 'run'
@(
    "name = $tournament"
    "board = 15"
    "rule = 0"
    "opening = openings"
    "engines = $($engineFiles -join ' ')"
    # server.py tests this value's truth as a string, so "0" would mean a round robin; empty is the gauntlet.
    "tournament ="
    "time_turn = $TimeTurnMs"
    "time_match = $TimeMatchMs"
    "tolerance = $ToleranceMs"
    "memory = $MemoryBytes"
    "real_time_pos = 0"
    "real_time_message = 0"
    "remote ="
) | Set-Content -Encoding ASCII (Join-Path $server "tournament\$tournament.txt")

$hash = { param($p) (Get-FileHash $p -Algorithm SHA256).Hash.Substring(0, 16) }
$total = 2 * $lines.Count * $opponents.Count
$revision = (git -C $repo rev-parse --short HEAD 2>$null)
$dirty = @(git -C $repo status --porcelain 2>$null).Count
@(
    "harness: GomocupJudge $(git -C $Judge rev-parse --short HEAD 2>$null) ($Judge)"
    "engine: $engine"
    "engine hash: $(& $hash $engine)"
    "package md5: $((Get-FileHash $zip -Algorithm MD5).Hash.ToLower())"
    "engine settings: $(if ($Flags) { $Flags } else { 'defaults (Config::default)' })"
    ($opponents | ForEach-Object { "opponent: $_ (hash $(& $hash $_))" })
    "openings file: $Openings"
    "openings hash: $(& $hash $Openings); used: $($lines.Count)"
    "games: $total; turn_ms: $TimeTurnMs; match_ms: $TimeMatchMs; tolerance_ms: $ToleranceMs; memory_bytes: $MemoryBytes"
    "clients: $Clients"
    "revision: $revision; uncommitted files: $dirty"
    "trace: $trace"
    "started: $(Get-Date -Format s)"
) | Set-Content (Join-Path $run 'run-info.txt')

# Rewrites the Judge's records into match.ps1's layout, one directory per opponent (vs_<name>), so the readers
# of that layout - results.csv, pNN_slap5_first.psq / pNN_opponent_first.psq, run-info.txt's `opponent:` and
# `pairs:` lines, and <engine>.log with START and DEBUG lines in play order - work unchanged.
# "first" means black, the colour of the first opening stone. The Judge's player 1 is the side to move after
# the opening, so slap5 is black when it is player 1 of an even-length opening or player 2 of an odd one.
function Export-MatchLayout {
    $results = Join-Path $server "result\$tournament"
    $statePath = Join-Path $results 'state.txt'
    if (-not (Test-Path $statePath)) { return }
    $stoneCounts = @($lines | ForEach-Object { [int](@($_ -split ',' | Where-Object { $_.Trim() }).Count / 2) })

    # The engine's own moves and log lines per process; the Judge starts one process per game. A pid can be
    # reused within a run, so every `engine start` line opens a new entry.
    $byPid = @{}
    $current = @{}
    if (Test-Path $trace) {
        foreach ($l in Get-Content $trace) {
            if ($l -notmatch '^\d+ pid=(\d+) (recv|sent|info) (.*)$') { continue }
            $id, $dir, $text = $Matches[1], $Matches[2], $Matches[3]
            if (($dir -eq 'info' -and $text -eq 'engine start') -or -not $current.ContainsKey($id)) {
                $current[$id] = "$id#$($byPid.Count)"
                $byPid[$current[$id]] = @{ moves = @(); log = @() }
            }
            $entry = $byPid[$current[$id]]
            if ($dir -eq 'recv' -and $text -like 'START*') { $entry.log += $text }
            elseif ($dir -eq 'sent' -and $text -like 'DEBUG*') { $entry.log += $text }
            elseif ($dir -eq 'sent' -and $text -match '^(\d+),(\d+)$') {
                $entry.moves += '{0},{1}' -f ([int]$Matches[1] + 1), ([int]$Matches[2] + 1)
            }
        }
    }

    $games = foreach ($row in Get-Content $statePath) {
        if ($row -notmatch '^\((\d+), (\d+), (\d+)\)\t(\d+)\t(\d+)') { continue }
        $p1, $p2, $round, $result, $endWith = [int]$Matches[1], [int]$Matches[2], [int]$Matches[3], [int]$Matches[4], [int]$Matches[5]
        $psq = Get-ChildItem $results -Filter ("{0}_{1}_{2}_*.psq" -f $round, $p1, $p2) | Select-Object -First 1
        if (-not $psq) { continue }
        $moves = @(Get-Content $psq.FullName | Where-Object { $_ -match '^\d+,\d+,\d+$' } | ForEach-Object { ($_ -split ',')[0..1] -join ',' })
        $stones = $stoneCounts[$round]
        $slap5IsP1 = $p1 -eq 0
        $ours = @(for ($i = $stones + $(if ($slap5IsP1) { 0 } else { 1 }); $i -lt $moves.Count; $i += 2) { $moves[$i] })
        $winner = switch ($result) { 1 { $p1 } 2 { $p2 } default { -1 } }
        [pscustomobject]@{
            Opponent = if ($slap5IsP1) { $p2 } else { $p1 }
            Pair = $round
            Black = $slap5IsP1 -xor ($stones % 2 -eq 1)
            Moves = $moves.Count
            Outcome = if ($winner -lt 0) { 'draw' } elseif ($winner -eq 0) { 'win' } else { 'loss' }
            EndBy = if ($endWith -eq 0 -and $winner -lt 0) { 'full board' } else { @('five', 'foul', 'timeout', 'illegal', 'crash')[$endWith] }
            Ours = $ours
            Psq = $psq.FullName
        }
    }

    # A process's moves identify its game: all of the record's moves for our side, or those with one move more
    # or fewer where the clock cut the last one off. A game takes a log only when exactly one process fits it
    # best and no other game fits that process; otherwise it gets none, as repeated openings can replay a game.
    $fit = {
        param($a, $b)
        $a, $b = @($a), @($b)
        $n = [Math]::Min($a.Count, $b.Count)
        if ($n -lt 1 -or [Math]::Abs($a.Count - $b.Count) -gt 1) { return -1 }
        if (($a[0..($n - 1)] -join ' ') -ne ($b[0..($n - 1)] -join ' ')) { return -1 }
        [int]($a.Count -ne $b.Count)
    }
    $assigned = @{}
    $claims = @{}
    foreach ($g in $games) {
        $ranked = @(foreach ($key in $byPid.Keys) {
            $r = & $fit $byPid[$key].moves $g.Ours
            if ($r -ge 0) { [pscustomobject]@{ Key = $key; Rank = $r } }
        })
        if ($ranked.Count -eq 0) { continue }
        $top = ($ranked | Measure-Object Rank -Minimum).Minimum
        $keys = @($ranked | Where-Object Rank -eq $top | ForEach-Object Key)
        if ($keys.Count -ne 1) { continue }
        $assigned[$g.Psq] = $keys[0]
        $claims[$keys[0]] = 1 + [int]$claims[$keys[0]]
    }
    $logFor = {
        param($g)
        $key = $assigned[$g.Psq]
        if ($key -and $claims[$key] -eq 1) { return $byPid[$key].log }
        @('START 15')
    }

    for ($k = 1; $k -le $opponents.Count; $k++) {
        $opp = $opponents[$k - 1]
        $oppName = [IO.Path]::GetFileNameWithoutExtension($opp)
        $dir = Join-Path $run "vs_$oppName"
        New-Item -ItemType Directory -Force $dir | Out-Null
        $mine = @($games | Where-Object Opponent -eq $k | Sort-Object Pair, @{ Expression = { -not $_.Black } })
        $log = @()
        $rows = foreach ($g in $mine) {
            $name = 'p{0:D2}_{1}_first.psq' -f $g.Pair, $(if ($g.Black) { 'slap5' } else { 'opponent' })
            Copy-Item $g.Psq (Join-Path $dir $name)
            $log += & $logFor $g
            [pscustomobject]@{ Pair = $g.Pair; Opening = $g.Pair; Slap5First = $g.Black; ExitCode = ''; Moves = $g.Moves; Outcome = $g.Outcome; EndBy = $g.EndBy }
        }
        $rows | Export-Csv -NoTypeInformation (Join-Path $dir 'results.csv')
        $log | Set-Content (Join-Path $dir ((Split-Path $engine -Leaf) + '.log'))
        $info = @(Get-Content (Join-Path $run 'run-info.txt') | Where-Object { $_ -notlike 'opponent: *' })
        $info += "opponent: $opp"
        $info += "pairs: $($lines.Count); openings: $((0..($lines.Count - 1)) -join ',')"
        $info | Set-Content (Join-Path $dir 'run-info.txt')
        $tally = { param($rs) '{0}W {1}L {2}D' -f @($rs | Where-Object Outcome -eq 'win').Count, @($rs | Where-Object Outcome -eq 'loss').Count, @($rs | Where-Object Outcome -eq 'draw').Count }
        $ends = ($rows | Where-Object EndBy -ne 'five' | Group-Object EndBy | ForEach-Object { "$($_.Count) $($_.Name)" }) -join ', '
        Write-Host ("slap5 vs {0}: {1}  (as black: {2}; as white: {3}){4}" -f $oppName, (& $tally $rows),
            (& $tally ($rows | Where-Object Slap5First)), (& $tally ($rows | Where-Object { -not $_.Slap5First })), $(if ($ends) { "; not by five: $ends" } else { '' }))
    }
}

$procs = @()
try {
    $srv = Start-Process $Python -ArgumentList '-u', 'server.py', $tournament, $Port -WorkingDirectory $server `
        -RedirectStandardOutput (Join-Path $run 'server-out.txt') -RedirectStandardError (Join-Path $run 'server-err.txt') `
        -WindowStyle Hidden -PassThru
    $procs += $srv
    # Waits for the server's own start line rather than probing the port: a probe connection is registered
    # as a client that never finishes, and the server then never exits.
    $serverLog = Join-Path $server "result\$tournament\log.txt"
    $deadline = (Get-Date).AddSeconds(15)
    while (-not ((Test-Path $serverLog) -and (Select-String -Path $serverLog -Pattern 'Server started' -Quiet))) {
        if ($srv.HasExited) { throw "server exited; see $run\server-err.txt" }
        if ((Get-Date) -gt $deadline) { throw "server did not start; see $run\server-err.txt" }
        Start-Sleep -Milliseconds 200
    }
    for ($i = 0; $i -lt $Clients; $i++) {
        $dir = Join-Path $run "client$i"
        $c = Start-Process $Python -ArgumentList '-u', "`"$(Join-Path $Judge 'client\client.py')`"", '--host', 'localhost', '--port', $Port, '--dir', "`"$dir`"" `
            -RedirectStandardOutput (Join-Path $run "client$i-out.txt") -RedirectStandardError (Join-Path $run "client$i-err.txt") `
            -WindowStyle Hidden -PassThru
        # Engines inherit the client's affinity, so both players of a game share one core, as at Gomocup.
        $c.ProcessorAffinity = [IntPtr](1 -shl ($i + 1))
        $procs += $c
    }

    $results = Join-Path $server "result\$tournament"
    $seen = -1
    while (-not $srv.HasExited) {
        $done = @(Get-ChildItem $results -Filter *.psq -ErrorAction SilentlyContinue).Count
        if ($done -ne $seen) { Write-Host "games finished: $done/$total"; $seen = $done }
        Start-Sleep -Seconds 5
    }
    $done = @(Get-ChildItem $results -Filter *.psq -ErrorAction SilentlyContinue).Count
    Write-Host "games finished: $done/$total"
    if (Test-Path (Join-Path $results '_result.txt')) { Get-Content (Join-Path $results '_result.txt') }
    if (Test-Path (Join-Path $results 'los.txt')) { Get-Content (Join-Path $results 'los.txt') }
    Export-MatchLayout
    Write-Host "results: $run"
}
finally {
    foreach ($p in $procs) { if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue } }
    # Only engines started from this run's directories; other runs and Piskvork sessions are left alone.
    Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -and $_.ExecutablePath.StartsWith($run, [StringComparison]::OrdinalIgnoreCase) } |
        ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
}
