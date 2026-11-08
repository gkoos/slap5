$root = Split-Path (Split-Path $PSScriptRoot)
$match = Join-Path $root 'engine\scripts\match.ps1'
$open = Join-Path $root 'engine\openings\gomocup-freestyle15-2026.txt'
$outroot = Join-Path $root 'logs\pool-chain'
New-Item -ItemType Directory -Force $outroot | Out-Null
$summary = Join-Path $outroot 'summary.txt'
Add-Content -Path $summary -Value ('chain started ' + (Get-Date -Format s))
while (Get-Process -Id 62248 -ErrorAction SilentlyContinue) { Start-Sleep -Seconds 30 }
Add-Content -Path $summary -Value ('kanec pool finished, chain starting ' + (Get-Date -Format s))
$names = @('pela','carbon','tito','pentazen','rapfi-avx2','ego','mushroom')
$paths = @('external\pela\pbrain-pela.exe','external\carbon\pbrain-carbon.exe','external\tito\bin\pbrain-Tito2014.exe','external\pentazen\bin\pbrain-PentaZen21_15.exe','external\rapfi\bin\pbrain-rapfi_avx2.exe','external\ego\pbrain-ego-zero-v1.exe','external\mushroom\pbrain-mushroom.exe')
$i = 0
foreach ($name in $names)
{
  $opp = Join-Path $root $paths[$i]
  $i = $i + 1
  $dir = Join-Path $outroot $name
  New-Item -ItemType Directory -Force $dir | Out-Null
  $log = Join-Path $dir 'console.out.txt'
  $csv = Get-ChildItem $dir -Recurse -Filter results.csv -ErrorAction SilentlyContinue
  if ($csv) { Add-Content -Path $summary -Value ($name + ': already done, skipped'); continue }
  Add-Content -Path $summary -Value ($name + ': started ' + (Get-Date -Format s))
  & $match -Opponent $opp -Pairs 52 -Openings $open -OutDir $dir *> $log
  $csv = Get-ChildItem $dir -Recurse -Filter results.csv -ErrorAction SilentlyContinue
  if (-not $csv) { Add-Content -Path $summary -Value ($name + ': no results.csv'); continue }
  $r = Import-Csv $csv[0].FullName
  Add-Content -Path $summary -Value ($name + ': games=' + $r.Count)
  foreach ($grp in ($r | Group-Object Outcome))
  {
    $m = @($grp.Group | ForEach-Object { [int]$_.Moves })
    $avg = [math]::Round((($m | Measure-Object -Average).Average),0)
    $sh = @($m | Where-Object { $_ -lt 25 }).Count
    $mi = @($m | Where-Object { $_ -ge 25 -and $_ -le 100 }).Count
    $lo = @($m | Where-Object { $_ -gt 100 }).Count
    Add-Content -Path $summary -Value ('   ' + $grp.Name + ' n=' + $grp.Count + ' avgMoves=' + $avg + ' under25=' + $sh + ' 25to100=' + $mi + ' over100=' + $lo)
  }
}
Add-Content -Path $summary -Value ('chain finished ' + (Get-Date -Format s))
