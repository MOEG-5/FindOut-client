#!/usr/bin/env python3
"""Package the native executable as a Finder-launchable app and updater asset."""
import argparse
import pathlib
import plistlib
import shutil
import stat
import tomllib
import zipfile


def package(binary: pathlib.Path, output: pathlib.Path, arch: str) -> pathlib.Path:
    repo = pathlib.Path(__file__).resolve().parent.parent
    version = tomllib.loads((repo / "Cargo.toml").read_text())["package"]["version"]
    contents = output / "FindOut.app" / "Contents"
    (contents / "MacOS").mkdir(parents=True, exist_ok=True)
    executable = contents / "MacOS" / "findout-client"
    shutil.copy2(binary, executable)
    executable.chmod(0o755)
    metadata = (repo / "packaging/macos/Info.plist").read_text().replace("@VERSION@", version)
    plistlib.loads(metadata.encode())  # Fail before publishing malformed metadata.
    (contents / "Info.plist").write_text(metadata)
    updater = output / f"findout-client-macos-{arch}"
    shutil.copy2(executable, updater)
    archive = output / f"findout-client-macos-{arch}.zip"
    with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as bundle:
        for path in sorted((output / "FindOut.app").rglob("*")):
            bundle.write(path, path.relative_to(output))
    # ZIP preserves the executable bit so Finder can launch the extracted app.
    with zipfile.ZipFile(archive) as bundle:
        mode = bundle.getinfo("FindOut.app/Contents/MacOS/findout-client").external_attr >> 16
        if not mode & stat.S_IXUSR:
            raise RuntimeError("Packaged app is not executable")
    return archive


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=pathlib.Path, default=pathlib.Path("target/release/findout-client"))
    parser.add_argument("--output", type=pathlib.Path, default=pathlib.Path("dist"))
    parser.add_argument("--arch", required=True, choices=["aarch64", "x86_64"])
    args = parser.parse_args()
    print(package(args.binary, args.output, args.arch))
