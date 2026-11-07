# SPDX-License-Identifier: AGPL-3.0-or-later
"""Build Kitab for release, the same way locally and in release.yml.

Windows: the C runtime is linked in (Rust `+crt-static`, and MuPDF's MSBuild
projects forced to /MT through MSVC's `_CL_`, which comes after a project's
own flags). kitab.exe then needs no Visual C++ Redistributable: a clean
Windows (and winget's validation machines) runs it as it is. Debug builds
keep the default runtime; MuPDF's debug configuration links the debug DLLs.

Run: python tools/build_release.py [extra cargo args]
"""
import os
import subprocess
import sys

PACKAGES = ["-p", "kitab", "-p", "kitab-shell"]


def main() -> int:
    env = dict(os.environ)
    if sys.platform == "win32":
        flags = env.get("RUSTFLAGS", "")
        if "crt-static" not in flags:
            env["RUSTFLAGS"] = (flags + " -C target-feature=+crt-static").strip()
        env["_CL_"] = (env.get("_CL_", "") + " /MT").strip()
    cmd = ["cargo", "build", "--release", *PACKAGES, *sys.argv[1:]]
    print(" ".join(cmd), flush=True)
    return subprocess.call(cmd, env=env)


if __name__ == "__main__":
    sys.exit(main())
