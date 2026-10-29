import ctypes as c
from ctypes import wintypes as w
import hashlib
import json
from pathlib import Path
import sys

image = Path(sys.argv[1]).resolve()
out = Path(sys.argv[2]).resolve()
root = Path.cwd().resolve()
assert image.is_relative_to(root) and out.is_relative_to(root)
assert image.is_file() and not out.exists()

class Symbol(c.Structure):
    _fields_ = [
        ('SizeOfStruct', w.ULONG), ('TypeIndex', w.ULONG),
        ('Reserved', c.c_uint64 * 2), ('Index', w.ULONG), ('Size', w.ULONG),
        ('ModBase', c.c_uint64), ('Flags', w.ULONG), ('Value', c.c_uint64),
        ('Address', c.c_uint64), ('Register', w.ULONG), ('Scope', w.ULONG),
        ('Tag', w.ULONG), ('NameLen', w.ULONG), ('MaxNameLen', w.ULONG),
        ('Name', c.c_char * 1),
    ]

kernel = c.WinDLL('kernel32', use_last_error=True)
kernel.GetCurrentProcess.restype = w.HANDLE
process = kernel.GetCurrentProcess()
dbg = c.WinDLL('dbghelp', use_last_error=True)
dbg.SymSetOptions.argtypes = [w.DWORD]
dbg.SymSetOptions.restype = w.DWORD
dbg.SymInitializeW.argtypes = [w.HANDLE, w.LPCWSTR, w.BOOL]
dbg.SymInitializeW.restype = w.BOOL
dbg.SymLoadModuleExW.argtypes = [w.HANDLE, w.HANDLE, w.LPCWSTR, w.LPCWSTR, c.c_uint64, w.DWORD, c.c_void_p, w.DWORD]
dbg.SymLoadModuleExW.restype = c.c_uint64
callback_type = c.WINFUNCTYPE(w.BOOL, c.POINTER(Symbol), w.ULONG, c.c_void_p)
dbg.SymEnumSymbols.argtypes = [w.HANDLE, c.c_uint64, c.c_char_p, callback_type, c.c_void_p]
dbg.SymEnumSymbols.restype = w.BOOL
dbg.SymCleanup.argtypes = [w.HANDLE]
dbg.SymCleanup.restype = w.BOOL
# Только соседний PDB; без symbol server и системного search path.
dbg.SymSetOptions(0x00000002 | 0x00000004 | 0x00000200 | 0x00080000 | 0x00001000)
if not dbg.SymInitializeW(process, str(image.parent), False):
    raise c.WinError(c.get_last_error())
rows = []
errors = []
@callback_type
def collect(ptr, size, context):
    try:
        info = ptr.contents
        if info.NameLen > 65536:
            raise ValueError('Некорректная длина symbol')
        name = c.string_at(c.addressof(info) + Symbol.Name.offset, info.NameLen).decode('utf8', 'replace')
        rows.append({'name': name, 'rva': info.Address - info.ModBase, 'size': int(info.Size), 'tag': int(info.Tag)})
        return True
    except BaseException as error:
        errors.append(str(error))
        return False
try:
    base = dbg.SymLoadModuleExW(process, None, str(image), None, 0, 0, None, 0)
    if not base:
        raise c.WinError(c.get_last_error())
    if not dbg.SymEnumSymbols(process, base, None, collect, None):
        raise c.WinError(c.get_last_error())
    assert not errors, errors
    out.parent.mkdir(parents=True, exist_ok=True)
    result = {'image': image.relative_to(root).as_posix(), 'sha256': hashlib.sha256(image.read_bytes()).hexdigest(), 'image_base': base, 'symbols': rows}
    out.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding='utf8')
    print(json.dumps({'symbols': len(rows), 'selected': [r for r in rows if any(s in r['name'] for s in ['own_free_wall', 'current_for_alloc', 'current_for_dealloc'])]}, ensure_ascii=False, indent=2))
finally:
    dbg.SymCleanup(process)
