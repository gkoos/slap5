while (-not (Get-ChildItem 'c:\projects\high5\logs\pool-chain\tito' -Recurse -Filter results.csv -ErrorAction SilentlyContinue)) { Start-Sleep -Seconds 15 }
Get-CimInstance Win32_Process -Filter "Name='powershell.exe'" | Where-Object { $_.CommandLine -like '*pool_chain.ps1*' } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
Get-Process -Name 'piskvork','pbrain-*' -ErrorAction SilentlyContinue | Stop-Process -Force
Set-Content -Path 'c:\projects\high5\logs\pool-chain\stopped.txt' -Value ('stopped after tito at ' + (Get-Date -Format s))
