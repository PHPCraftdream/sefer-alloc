$sig = @'
using System;
using System.Runtime.InteropServices;
public static class JP {
  [DllImport("kernel32.dll")] public static extern bool IsProcessInJob(IntPtr p, IntPtr j, out bool r);
  [DllImport("kernel32.dll")] public static extern IntPtr GetCurrentProcess();
  [DllImport("kernel32.dll", SetLastError=true)] public static extern bool QueryInformationJobObject(IntPtr job, int cls, byte[] buf, int len, out int ret);
}
'@
Add-Type -TypeDefinition $sig
$in = $false
[void][JP]::IsProcessInJob([JP]::GetCurrentProcess(), [IntPtr]::Zero, [ref]$in)
"in_job=$in"
if ($in) {
  $buf = New-Object byte[] 8
  $ret = 0
  $ok = [JP]::QueryInformationJobObject([IntPtr]::Zero, 15, $buf, 8, [ref]$ret)
  if ($ok) {
    $flags = [BitConverter]::ToUInt32($buf, 0)
    $rate = [BitConverter]::ToUInt32($buf, 4)
    "cpu_rate_flags=0x{0:X} cpu_rate_raw={1} (hard cap percent={2})" -f $flags, $rate, ($rate/100)
  } else { "cpu_rate_query_failed gle=" + [Runtime.InteropServices.Marshal]::GetLastWin32Error() }
  $buf2 = New-Object byte[] 144
  $ok2 = [JP]::QueryInformationJobObject([IntPtr]::Zero, 9, $buf2, 144, [ref]$ret)
  if ($ok2) {
    $lf = [BitConverter]::ToUInt32($buf2, 16)
    $pc = [BitConverter]::ToUInt32($buf2, 40)
    "limit_flags=0x{0:X} priority_class_field={1}" -f $lf, $pc
  }
}
