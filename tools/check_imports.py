# SPDX-License-Identifier: AGPL-3.0-or-later
"""Fail if a Windows binary imports the Visual C++ runtime.

Reads the PE import table of each file given and lists the DLLs it loads.
VCRUNTIME*, MSVCP*, UCRTBASE and api-ms-win-crt-* mean the binary needs the
Visual C++ Redistributable, which a clean Windows doesn't have (and winget
validation runs on one). tools/build_release.py links the runtime in.

Run: python tools/check_imports.py target/release/kitab.exe [more.dll]
"""
import struct
import sys

BANNED = ("vcruntime", "msvcp", "ucrtbase", "api-ms-win-crt-", "msvcr")


def imports(path: str) -> list[str]:
    data = open(path, "rb").read()
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    if data[pe:pe + 4] != b"PE\0\0":
        raise ValueError(f"{path}: not a PE file")
    coff = pe + 4
    sections = struct.unpack_from("<H", data, coff + 2)[0]
    opt_size = struct.unpack_from("<H", data, coff + 16)[0]
    opt = coff + 20
    magic = struct.unpack_from("<H", data, opt)[0]
    dirs = opt + (112 if magic == 0x20B else 96)
    import_rva = struct.unpack_from("<I", data, dirs + 8)[0]
    table = []
    sec = opt + opt_size
    for i in range(sections):
        s = sec + i * 40
        vsize, vaddr, rsize, raddr = struct.unpack_from("<IIII", data, s + 8)
        table.append((vaddr, max(vsize, rsize), raddr))

    def off(rva: int) -> int:
        for vaddr, size, raddr in table:
            if vaddr <= rva < vaddr + size:
                return rva - vaddr + raddr
        raise ValueError(f"RVA {rva:#x} outside every section")

    names = []
    at = off(import_rva)
    while True:
        name_rva = struct.unpack_from("<I", data, at + 12)[0]
        if name_rva == 0:
            break
        o = off(name_rva)
        names.append(data[o:data.index(b"\0", o)].decode("ascii"))
        at += 20
    return names


def main() -> int:
    bad = False
    for path in sys.argv[1:]:
        dlls = imports(path)
        print(f"{path}: {', '.join(dlls)}")
        hits = [d for d in dlls if d.lower().startswith(BANNED)]
        if hits:
            print(f"  needs the Visual C++ runtime: {', '.join(hits)}", file=sys.stderr)
            bad = True
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
