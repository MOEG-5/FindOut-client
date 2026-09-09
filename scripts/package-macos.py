#!/usr/bin/env python3
"""Package the native executable as a Finder-launchable app and updater asset."""
import argparse
import pathlib
import plistlib
import shutil
import subprocess
import sys
import tempfile
import time
import tomllib


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
    return disk_image(output / "FindOut.app", output, arch)


def disk_image(app: pathlib.Path, output: pathlib.Path, arch: str) -> pathlib.Path:
    output.mkdir(parents=True, exist_ok=True)
    image = (output / f"findout-client-macos-{arch}.dmg").resolve()
    with tempfile.TemporaryDirectory(prefix="findout-dmg-stage-") as directory:
        stage = pathlib.Path(directory)
        shutil.copytree(app, stage / "FindOut.app", symlinks=True)
        (stage / "Install FindOut.txt").write_text(
            "Open FindOut.app, then click Install to install for your account.\n"
            "After installation, eject this disk image.\n"
            "FindOut lives in your menu bar. Press Option+Space to open it.\n"
        )
        subprocess.run(["hdiutil", "create", "-srcfolder", str(stage),
                        "-volname", "FindOut", "-fs", "HFS+", "-format", "UDZO",
                        "-ov", str(image)], check=True)
    subprocess.run(["hdiutil", "verify", str(image)], check=True)
    return image


def verify_disk_image(image: pathlib.Path, app: pathlib.Path, launch: bool) -> None:
    """Validate the delivered, read-only volume rather than just the staging app."""
    with tempfile.TemporaryDirectory(prefix="findout-dmg-mount-") as directory:
        mount = pathlib.Path(directory)
        subprocess.run(["hdiutil", "attach", str(image), "-readonly", "-nobrowse",
                        "-mountpoint", str(mount)], check=True)
        try:
            for relative in ["Contents/Info.plist", "Contents/MacOS/findout-client"]:
                if (mount / "FindOut.app" / relative).read_bytes() != (app / relative).read_bytes():
                    raise RuntimeError(f"Disk image changed {relative}")
            executable = mount / "FindOut.app/Contents/MacOS/findout-client"
            if not executable.stat().st_mode & 0o111:
                raise RuntimeError("Mounted app is not executable")
            subprocess.run(["codesign", "--verify", "--strict", str(executable)], check=True)
            if launch:
                smoke_test(mount)
        finally:
            subprocess.run(["hdiutil", "detach", str(mount)], check=True)
    print("Mounted DMG app content, executable permissions and signature verified")


def smoke_test(output: pathlib.Path) -> None:
    """Check startup only in the disposable macOS runner session."""
    executable = (output / "FindOut.app/Contents/MacOS/findout-client").resolve()
    with tempfile.TemporaryFile(mode="w+") as log:
        process = subprocess.Popen([str(executable)], stdin=subprocess.DEVNULL,
                                   stdout=log, stderr=subprocess.STDOUT)
        try:
            time.sleep(5)
            if process.poll() is not None:
                log.seek(0)
                raise RuntimeError(f"Packaged app exited during startup: {log.read()}")
        finally:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
    print("Packaged macOS app stayed running through startup")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", type=pathlib.Path, help="Repackage an existing app without changing its binary")
    parser.add_argument("--expected-version", help="Require this version when repackaging an app")
    parser.add_argument("--binary", type=pathlib.Path, default=pathlib.Path("target/release/findout-client"))
    parser.add_argument("--output", type=pathlib.Path, default=pathlib.Path("dist"))
    parser.add_argument("--arch", required=True, choices=["aarch64", "x86_64"])
    parser.add_argument("--smoke-test", action="store_true",
                        help="Launch briefly; use only in a disposable macOS GUI session")
    args = parser.parse_args()
    if sys.platform != "darwin":
        parser.error("DMG packaging requires macOS")
    if args.app:
        metadata = plistlib.loads((args.app / "Contents/Info.plist").read_bytes())
        if metadata.get("CFBundleIdentifier") != "app.findout.client":
            parser.error("Expected a FindOut app bundle")
        if args.expected_version and metadata.get("CFBundleShortVersionString") != args.expected_version:
            parser.error("App version does not match the requested release")
        app = args.app
        image = disk_image(app, args.output, args.arch)
    else:
        image = package(args.binary, args.output, args.arch)
        app = args.output / "FindOut.app"
    verify_disk_image(image, app, args.smoke_test)
    print(image)
