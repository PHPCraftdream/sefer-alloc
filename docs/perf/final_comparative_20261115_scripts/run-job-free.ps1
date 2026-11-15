# Runs a command line OUTSIDE any Job Object of this session (Win32_Process.Create via WMI), logs to a file,
# and blocks until the created process exits. Usage:
#   run-job-free.ps1 -WorkDir <dir> -LogFile <file> -CmdLine "<command line>"
param(
  [Parameter(Mandatory=$true)][string]$WorkDir,
  [Parameter(Mandatory=$true)][string]$LogFile,
  [Parameter(Mandatory=$true)][string]$CmdLine
)
$ErrorActionPreference = 'Stop'
$cmdFile = [System.IO.Path]::ChangeExtension($LogFile, '.cmd')
$lines = @(
  '@echo off',
  "cd /d `"$WorkDir`"",
  "$CmdLine > `"$LogFile`" 2>&1",
  "echo EXIT=%ERRORLEVEL% >> `"$LogFile`""
)
[System.IO.File]::WriteAllLines($cmdFile, $lines)
$r = Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{ CommandLine = "cmd.exe /c `"$cmdFile`""; CurrentDirectory = $WorkDir }
if ($r.ReturnValue -ne 0) { throw "Win32_Process.Create failed: $($r.ReturnValue)" }
"started pid=$($r.ProcessId) log=$LogFile"
Wait-Process -Id $r.ProcessId
"finished pid=$($r.ProcessId)"
