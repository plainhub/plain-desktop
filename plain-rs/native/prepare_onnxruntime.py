#!/usr/bin/env python3
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import urllib.request
import zipfile

VERSION = "1.30.0"
SOURCE_COMMIT = "f2c39fe2f838cf35ce7da92824f5a5e3ee6e88a7"
DIGESTS = {
    "osx-arm64": "6ebb5062a934537c352937821f9fe9718e7de1a2db1122a93dd363ffd53a7012",
    "linux-x64": "a5ed5a3cac51fbb2e90da632ae43d19212faaa20e76484e62bcb7c23ddb3b3fd",
    "linux-aarch64": "e16a27a8ed330bbc698df7330b0cf56e722f354e3bcc92118682c74ef3c3e3da",
    "win-x64": "c6ba983baf5681af108599675d2a89c2d145512d02de28aed0bff177cd0ba949",
    "win-arm64": "e53db8a50b23ae35be901cc93428baf997dc8d420333b097b2eae53d3ea9f2d3",
}


def run(args, **kwargs):
    subprocess.run([str(a) for a in args], check=True, **kwargs)


def download(url, dest):
    if dest.is_file():
        return
    partial = dest.with_suffix(dest.suffix + ".part")
    with urllib.request.urlopen(url, timeout=60) as response, partial.open("wb") as output:
        shutil.copyfileobj(response, output)
    partial.replace(dest)


def file_hash(path):
    digest = hashlib.sha256()
    with path.open("rb") as data:
        for chunk in iter(lambda: data.read(65536), b""):
            digest.update(chunk)
    return digest.hexdigest()


def acquire_lock(path):
    lock = path.open("a+b")
    if os.name == "nt":
        import msvcrt
        if lock.tell() == 0:
            lock.write(b"0")
            lock.flush()
        lock.seek(0)
        msvcrt.locking(lock.fileno(), msvcrt.LK_LOCK, 1)
    else:
        import fcntl
        fcntl.flock(lock.fileno(), fcntl.LOCK_EX)
    return lock


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--target", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--source", action="store_true")
    args = parser.parse_args()
    target = args.target
    source = args.source or "android" in target or "apple-ios" in target
    cache = Path(os.environ.get("PLAIN_ONNX_CACHE", Path.home() / ".cache/plain-onnxruntime")) / VERSION
    cache.mkdir(parents=True, exist_ok=True)
    lock = acquire_lock(cache / ("prepare-" + target + ".lock"))
    args.output.mkdir(parents=True, exist_ok=True)
    name = "onnxruntime.dll" if "windows" in target else "libonnxruntime.a" if "apple-ios" in target else "libonnxruntime.dylib" if "apple" in target else "libonnxruntime.so"
    destination = args.output / name
    stamp = args.output / "runtime.json"
    identity = {"version": VERSION, "target": target, "source": source, "recipe": file_hash(Path(__file__))}
    if destination.is_file() and stamp.is_file():
        recorded = json.loads(stamp.read_text())
        if recorded.get("identity") == identity and recorded.get("sha256") == file_hash(destination) and all((args.output / name).is_file() for name in ("LICENSE", "ThirdPartyNotices.txt")):
            return
    if source:
        checkout = cache / "source"
        checkout_lock = acquire_lock(cache / "checkout.lock")
        try:
            if not checkout.exists():
                run(["git", "clone", "--depth", "1", "--branch", "v" + VERSION,
                     "--recurse-submodules", "--shallow-submodules", "https://github.com/microsoft/onnxruntime.git", checkout])
        finally:
            checkout_lock.close()
        commit = subprocess.check_output(["git", "-C", str(checkout), "rev-parse", "HEAD"], text=True).strip()
        if commit != SOURCE_COMMIT:
            raise RuntimeError("ONNX Runtime source revision mismatch")
        for legal_name in ("LICENSE", "ThirdPartyNotices.txt"):
            shutil.copy2(checkout / legal_name, args.output / legal_name)
        build = cache / target
        cmd = [sys.executable, checkout / "tools/ci_build/build.py", "--build_dir", build,
               "--config", "Release", "--update", "--build", "--skip_tests",
               "--parallel", os.environ.get("PLAIN_ONNX_BUILD_JOBS", "4"),
               "--cmake_extra_defines", "onnxruntime_BUILD_UNIT_TESTS=OFF", "onnxruntime_USE_TELEMETRY=OFF", "CMAKE_POLICY_VERSION_MINIMUM=3.5"]
        if "android" in target:
            abi = {"aarch64-linux-android": "arm64-v8a", "armv7-linux-androideabi": "armeabi-v7a",
                   "i686-linux-android": "x86", "x86_64-linux-android": "x86_64"}[target]
            sdk = os.environ.get("ANDROID_HOME") or os.environ.get("ANDROID_SDK_ROOT")
            ndk = os.environ.get("ANDROID_NDK_HOME")
            if not sdk or not ndk:
                raise RuntimeError("ANDROID_HOME and ANDROID_NDK_HOME are required for the source build")
            cmd += ["--android", "--android_sdk_path", sdk, "--android_ndk_path", ndk,
                    "--android_abi", abi, "--android_api", "26", "--build_shared_lib", "--android_cpp_shared", "--use_xnnpack"]
        elif "apple-ios" in target:
            sdk = "iphonesimulator" if target.endswith("-sim") or target.startswith("x86_64") else "iphoneos"
            cmd += ["--ios", "--cmake_generator", "Xcode", "--apple_sysroot", sdk, "--osx_arch", "x86_64" if target.startswith("x86_64") else "arm64",
                    "--apple_deploy_target", "15.0", "--build_apple_framework", "--use_coreml", "--use_xnnpack"]
        else:
            cmd += ["--build_shared_lib", "--use_xnnpack"]
            if "apple" in target:
                cmd += ["--use_coreml", "--osx_arch", "x86_64" if target.startswith("x86_64") else "arm64"]
        cmd += ["--skip_submodule_sync"]
        run(cmd)
        if "apple-ios" in target:
            archives = sorted((build / "Release").rglob("onnxruntime.framework/onnxruntime"))
            if len(archives) != 1:
                raise RuntimeError("ONNX Runtime static framework missing or ambiguous")
            shutil.copy2(archives[0], destination)
        else:
            libraries = [p for p in (build / "Release").rglob(name + "*") if p.is_file() and not p.is_symlink()]
            if not libraries:
                raise RuntimeError("ONNX Runtime library missing after source build")
            shutil.copy2(libraries[0], destination)
    else:
        slug = "osx-arm64" if target == "aarch64-apple-darwin" else "linux-aarch64" if target == "aarch64-unknown-linux-gnu" else "linux-x64" if target == "x86_64-unknown-linux-gnu" else "win-x64" if target == "x86_64-pc-windows-msvc" else "win-arm64" if target == "aarch64-pc-windows-msvc" else None
        if not slug:
            raise RuntimeError("Use --source for this target: " + target)
        suffix = ".zip" if "windows" in target else ".tgz"
        asset = f"onnxruntime-{slug}-{VERSION}{suffix}"
        archive = cache / asset
        download(f"https://github.com/microsoft/onnxruntime/releases/download/v{VERSION}/{asset}", archive)
        if file_hash(archive) != DIGESTS[slug]:
            raise RuntimeError("ONNX Runtime release checksum mismatch")
        if suffix == ".zip":
            with zipfile.ZipFile(archive) as packed:
                member = next(n for n in packed.namelist() if n.endswith("/lib/" + name))
                destination.write_bytes(packed.read(member))
                for legal_name in ("LICENSE", "ThirdPartyNotices.txt"):
                    legal_member = next(n for n in packed.namelist() if n.endswith("/" + legal_name))
                    (args.output / legal_name).write_bytes(packed.read(legal_member))
        else:
            with tarfile.open(archive) as packed:
                member = next(m for m in packed.getmembers() if m.isfile() and "/lib/" in m.name and name in m.name)
                with packed.extractfile(member) as input_file, destination.open("wb") as output:
                    shutil.copyfileobj(input_file, output)
                for legal_name in ("LICENSE", "ThirdPartyNotices.txt"):
                    legal_member = next(m for m in packed.getmembers() if m.isfile() and m.name.endswith("/" + legal_name))
                    with packed.extractfile(legal_member) as input_file:
                        (args.output / legal_name).write_bytes(input_file.read())
    digest = file_hash(destination)
    stamp.write_text(json.dumps({"identity": identity, "sha256": digest}))


if __name__ == "__main__":
    main()
