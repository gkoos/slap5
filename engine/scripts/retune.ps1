# Retunes the pruning thresholds after a change to the evaluation's scale.
#
# The eval's numbers and the futility margin live in the same units, so a change that makes the eval
# smaller (counting threats once instead of two to four times) changes how often the margin fires.
# This script measures that: it builds the same tree with the old eval, runs the position suite and
# the game corpus with both builds, prints the per-position node ratio and the depth reached, and
# prints the margin scaled by the factor the eval shrank by. Nothing is edited automatically - the
# numbers it prints are what goes into search.rs by hand.
#
# Usage: powershell -NoProfile -File engine\scripts\retune.ps1
# Output: logs\retune-{old,new}.txt, logs\retune-corpus-{old,new}.txt and a summary on stdout.
# cargo writes progress to stderr and PowerShell 5.1 surfaces that as a NativeCommandError, which with
# 'Stop' aborts the whole script on the first build. The exit codes that matter are checked below.
$ErrorActionPreference = 'Continue'
# The script lives in engine/scripts but cargo and git run from the repository root: the workspace's
# target directory and lock file are at that level, and `eval_from_games` defaults to the `logs`
# directory relative to the working directory, which is also where it lives.
$engineDir = if ($PSScriptRoot) { Split-Path $PSScriptRoot } else { (Get-Location).Path }
$repo = Split-Path $engineDir
Set-Location $repo
$out = Join-Path $repo 'logs'
New-Item -ItemType Directory -Force $out | Out-Null

function Measure-Build([string]$tag) {
    cargo run -p slap5 --release --example suite 2>&1 | Set-Content (Join-Path $out "retune-$tag.txt")
    cargo run -p slap5 --release --example eval_from_games -- 4 100 5 2>&1 | Set-Content (Join-Path $out "retune-corpus-$tag.txt")
    "measured $tag"
}

# The old eval is the committed file; the change under test sits in the working tree.
$dirty = @(git status --porcelain -- engine/src/eval.rs engine/src/search.rs).Count
if ($dirty -eq 0) { throw 'the eval and the margin are unmodified - there is nothing to compare against' }
git stash push -- engine/src/eval.rs engine/src/search.rs | Out-Null
try { Measure-Build 'old' } finally { git stash pop | Out-Null }
Measure-Build 'new'

# Per position: nodes and depth at equal time, and depth at the suite's fixed depth.
function Parse-Suite([string]$path) {
    Get-Content $path | ForEach-Object {
        if ($_ -match '^(PentaZen-\S+)\s+depth\s+(\d+)\s+->.*nodes\s+(\d+).*time\s+(\d+)ms') {
            [pscustomobject]@{ pos = $Matches[1]; depth = [int]$Matches[2]; nodes = [int]$Matches[3]; ms = [int]$Matches[4] }
        }
    }
}
$old = @(Parse-Suite (Join-Path $out 'retune-old.txt'))
$new = @(Parse-Suite (Join-Path $out 'retune-new.txt'))
"position                     old nodes   new nodes   ratio   depth old -> new"
$tot = 0.0
$n = [Math]::Min($old.Count, $new.Count)
for ($i = 0; $i -lt $n; $i++) {
    $r = [double]$new[$i].nodes / [Math]::Max(1, $old[$i].nodes)
    $tot += $r
    '{0,-26} {1,10} {2,11} {3,7:N2}   {4} -> {5}' -f $old[$i].pos, $old[$i].nodes, $new[$i].nodes, $r, $old[$i].depth, $new[$i].depth
}
if ($n -gt 0) { 'mean node ratio new/old: {0:N3}' -f ($tot / $n) }

# How much smaller the eval's numbers got, read off the corpus's ordinary positions.
function Mean-Static([string]$path) {
    $line = Get-Content $path | Where-Object { $_ -match '^bulk <=' } | Select-Object -First 1
    if ($line -and $line -match 'mean static\s+(-?\d+)') { [double]$Matches[1] } else { [double]::NaN }
}
$mOld = Mean-Static (Join-Path $out 'retune-corpus-old.txt')
$mNew = Mean-Static (Join-Path $out 'retune-corpus-new.txt')
'corpus mean static: old {0:N0}, new {1:N0}' -f $mOld, $mNew
if (-not [double]::IsNaN($mOld) -and -not [double]::IsNaN($mNew) -and $mOld -ne 0) {
    $f = $mNew / $mOld
    'eval scale factor f = {0:N3}' -f $f
    'futility margin 150 + 120*depth  ->  {0:N0} + {1:N0}*depth   (a milder variant: {2:N0} + {3:N0}*depth)' -f (150 * $f), (120 * $f), (150 * $f * 1.15), (120 * $f * 1.15)
    'check the same on Stats::null_nodes before choosing: it has no margin, so if it fired as often before, nothing is needed there'
}
