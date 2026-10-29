import ctypes
from ctypes import wintypes
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import time

root = Path.cwd().resolve()
repo = root / 'worktrees/native-mixed-judge'
dir = repo / 'docs/perf/native_mixed/cpu2'
assert os.name == 'nt'
assert not dir.exists(), 'Не перезаписывать diagnostic evidence'
image = root / '.rush/native-owner/mixed-target/release/examples/native_mixed_sefer.exe'
assert image.resolve().is_relative_to(root)
sha = lambda b: hashlib.sha256(b).hexdigest()
calibration = repo / 'docs/perf/native_mixed/calibration1'
identity = json.loads((calibration / 'identity.json').read_text())
binaries = json.loads((calibration / 'binaries.json').read_text())
assert sha(image.read_bytes()) == binaries['sefer']
for name, expected in identity['files'].items():
    assert sha((repo / name).read_bytes()) == expected
frozen = json.loads((calibration / 'frozen.json').read_text())
assert frozen['repeats'] == 241
# HANDLE принадлежит своему Popen child и остаётся открытым до конца query.
kernel32 = ctypes.WinDLL('kernel32', use_last_error=True)
get_times = kernel32.GetProcessTimes
get_times.argtypes = [wintypes.HANDLE] + [ctypes.POINTER(wintypes.FILETIME)] * 4
get_times.restype = wintypes.BOOL
get_cycles = kernel32.QueryProcessCycleTime
get_cycles.argtypes = [wintypes.HANDLE, ctypes.POINTER(ctypes.c_ulonglong)]
get_cycles.restype = wintypes.BOOL
value = lambda ft: (ft.dwHighDateTime << 32) | ft.dwLowDateTime
dir.mkdir(parents=True)
meta = {'captured_before_execution_ns': time.time_ns(), 'base': identity['base'],
        'binary_sha256': binaries['sefer'], 'source_identity_sha256': sha((calibration / 'identity.json').read_bytes()),
        'features': 'production', 'python': sys.version, 'platform': platform.platform(),
        'processes': 6, 'repeats': frozen['repeats'], 'new_build': False, 'candidate_AB': False}
(dir / 'identity.json').write_text(json.dumps(meta, indent=2))
(dir / 'cpu-diagnostic.py').write_text(Path(__file__).read_text(encoding='utf-8'), encoding='utf-8')
for index in range(6):
    start = time.perf_counter_ns()
    with subprocess.Popen([str(image), str(frozen['repeats'])], cwd=repo,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE) as child:
        stdout, stderr = child.communicate(timeout=120)
        parent_ns = time.perf_counter_ns() - start
        creation, exit, kernel, user = [wintypes.FILETIME() for _ in range(4)]
        cycles = ctypes.c_ulonglong()
        handle = wintypes.HANDLE(int(child._handle))
        if not get_times(handle, ctypes.byref(creation), ctypes.byref(exit), ctypes.byref(kernel), ctypes.byref(user)):
            raise ctypes.WinError(ctypes.get_last_error())
        if not get_cycles(handle, ctypes.byref(cycles)):
            raise ctypes.WinError(ctypes.get_last_error())
        row = {'index': index, 'pid': child.pid, 'status': child.returncode, 'parent_ns': parent_ns,
               'process_lifetime_ns': (value(exit) - value(creation)) * 100,
               'kernel_ns': value(kernel) * 100, 'user_ns': value(user) * 100,
               'cpu_cycles': cycles.value, 'stdout': stdout.decode(), 'stderr': stderr.decode()}
        with (dir / 'samples.ndjson').open('a', encoding='utf-8') as stream:
            stream.write(json.dumps(row) + '\n')
        assert child.returncode == 0, stderr.decode()
        parsed = json.loads(stdout)
        assert parsed['completed'] and parsed['repeats'] == frozen['repeats']
        assert parsed['segments_reserved_total'] > 0
        assert cycles.value > 0
assert sha(image.read_bytes()) == binaries['sefer']
for name, expected in identity['files'].items():
    assert sha((repo / name).read_bytes()) == expected
(dir / 'completed.json').write_text(json.dumps({'processes': 6, 'binary_drift': False, 'source_drift': False}))
print('CPU diagnostic complete: 6 exact immutable child processes; calibration FAIL unchanged.')
