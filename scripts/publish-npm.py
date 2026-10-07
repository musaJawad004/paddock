#!/usr/bin/env python3
"""Publish paddock-cli and its platform packages to npm.

    scripts/publish-npm.py 0.1.0 npm-bins

`npm-bins/npm-<platform>/` holds the binary for each platform, as the
release workflow downloads them. Each platform package declares `os` and
`cpu`, so npm installs only the one that fits; the main package lists them
all as optional dependencies and runs the right one (npm/paddock-cli).
Nothing is downloaded at install time.
"""

import json
import pathlib
import shutil
import subprocess
import sys

PLATFORMS = {
    "darwin-arm64": ("darwin", "arm64"),
    "darwin-x64": ("darwin", "x64"),
    "linux-x64": ("linux", "x64"),
    "linux-arm64": ("linux", "arm64"),
    "win32-x64": ("win32", "x64"),
}
COMMON = {
    "license": "MIT",
    "repository": {"type": "git", "url": "git+https://github.com/musaJawad004/paddock.git"},
    "homepage": "https://github.com/musaJawad004/paddock",
}


def publish(folder):
    subprocess.run(["npm", "publish", "--access", "public"], cwd=folder, check=True)


def main():
    version, bins = sys.argv[1], pathlib.Path(sys.argv[2])
    out = pathlib.Path("npm-out")
    shutil.rmtree(out, ignore_errors=True)

    for platform, (os_name, cpu) in PLATFORMS.items():
        folder = out / platform
        (folder / "bin").mkdir(parents=True)
        exe = "paddock.exe" if os_name == "win32" else "paddock"
        target = folder / "bin" / exe
        shutil.copy(bins / f"npm-{platform}" / exe, target)
        target.chmod(0o755)
        package = {
            "name": f"paddock-cli-{platform}",
            "version": version,
            "description": f"Paddock binary for {platform}. Install paddock-cli instead.",
            "os": [os_name],
            "cpu": [cpu],
            "files": ["bin"],
            **COMMON,
        }
        (folder / "package.json").write_text(json.dumps(package, indent=2) + "\n")
        publish(folder)

    main_folder = out / "paddock-cli"
    shutil.copytree("npm/paddock-cli", main_folder)
    package = json.loads((main_folder / "package.json").read_text())
    package["version"] = version
    package["optionalDependencies"] = {f"paddock-cli-{p}": version for p in PLATFORMS}
    (main_folder / "package.json").write_text(json.dumps(package, indent=2) + "\n")
    shutil.copy("README.md", main_folder / "README.md")
    publish(main_folder)


if __name__ == "__main__":
    main()
