"""Build unpublished Windows installers and standalone ZIPs. Never uploads files."""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def digest(path):
    with path.open("rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--roles", nargs="+", choices=("teacher", "student"), default=["teacher", "student"])
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if sys.platform != "win32":
        parser.error("Windows x64 build host required")
    output = args.output.resolve()
    if output.exists():
        parser.error("Output already exists; choose a new directory")
    if output == ROOT or ROOT.is_relative_to(output):
        parser.error("Output must not contain the source repository")
    env = os.environ.copy()
    path_key = next((key for key in env if key.upper() == "PATH"), "PATH")
    cargo_bin = Path.home() / ".cargo/bin"
    env[path_key] = str(cargo_bin) + os.pathsep + env.get(path_key, "")
    host = subprocess.check_output([str(cargo_bin / "rustc.exe"), "-vV"], env=env, encoding="utf-8")
    if "host: x86_64-pc-windows-msvc" not in host:
        parser.error("Expected x86_64 MSVC toolchain")
    # Fixed output lookup must use this workspace target, never another checkout.
    env["CARGO_TARGET_DIR"] = str(ROOT / "target")
    upstream = json.loads((ROOT / "installer/windows/UPSTREAM.json").read_text(encoding="utf-8"))
    installed_cli = json.loads((ROOT / "node_modules/@tauri-apps/cli/package.json").read_text(encoding="utf-8"))["version"]
    if installed_cli != upstream["cli_version"]:
        parser.error("Review the pinned installer template before upgrading Tauri CLI")
    source_files = [ROOT / "Cargo.lock", ROOT / "release/native-policy.json", *ROOT.glob("crates/offline-core/src/**/*.rs"),
                    *ROOT.glob("apps/*/src-tauri/src/*.rs"), *ROOT.glob("apps/*/src-tauri/*.json"), *ROOT.glob("apps/*/ui/*.html"), *ROOT.glob("apps/*/ui/*.js"), *ROOT.glob("crates/native-vault/src/*.rs"),
                    *ROOT.glob("crates/*/Cargo.toml"), ROOT / "scripts/build-ui.mjs", ROOT / "tools/package_windows.py",
                    *ROOT.glob("apps/shared-ui/*"), *ROOT.glob("installer/windows/*")]
    source_hashes = {str(p.relative_to(ROOT)).replace(chr(92), "/"):digest(p) for p in source_files if p.is_file()}
    output.mkdir(parents=True)
    report = {"format": 1, "created_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
              "channel": "development", "publication_approved": False, "authenticode_signed": False,
              "tests_run": False, "artifacts": [], "status": "building", "source_sha256": source_hashes}
    try:
        for role in dict.fromkeys(args.roles):
            app = ROOT / "apps" / role
            config = json.loads((app / "src-tauri/tauri.conf.json").read_text(encoding="utf-8"))
            result = subprocess.run([shutil.which("node"), str(ROOT / "node_modules/@tauri-apps/cli/tauri.js"),
                                     "build", "--bundles", "nsis", "--", "--locked"],
                                    cwd=app, env=env, capture_output=True, text=True, encoding="utf-8", errors="replace")
            (output / (role + "-build.log")).write_text(result.stdout + "\n" + result.stderr, encoding="utf-8")
            if result.returncode:
                raise RuntimeError(role + " installer build failed; see build log")
            generated = (ROOT / "target/release/nsis/x64/installer.nsi").read_text(encoding="utf-8")
            if 'DeleteAppDataCheckbox' in generated or 'RmDir /r "$APPDATA' in generated or 'RmDir /r "$LOCALAPPDATA' in generated:
                raise RuntimeError("Uninstaller must preserve account data")
            version = config["version"]
            installer = ROOT / "target/release/bundle/nsis" / (config["productName"] + "_" + version + "_x64-setup.exe")
            executable = ROOT / "target/release" / (role + ".exe")
            if not installer.is_file() or not executable.is_file():
                raise RuntimeError("Expected role-specific installer/executable not found")
            prefix = "thinkgrow-" + role + "-" + version + "-windows-x64"
            setup = output / (prefix + "-setup.exe")
            shutil.copyfile(installer, setup)
            portable = output / (prefix + "-standalone.zip")
            readme = ("생각쑥쑥 " + role + " 개발 빌드 — 정식 배포 전\n"
                      "ZIP 압축을 푼 뒤 thinkgrow-" + role + ".exe를 실행하세요.\n"
                      "Microsoft Edge WebView2 Runtime이 설치되어 있어야 합니다.\n"
                      "인터넷 없는 첫 설치에는 함께 생성된 Setup.exe를 사용하세요.\n"
                      "이 ZIP은 실행 파일의 무설치 배포물입니다. 사용자 자료는 Windows 계정의 앱 데이터에 저장되며 USB에 함께 이동하지 않습니다.\n"
                      "EXE/ZIP 삭제 또는 앱 제거로 학습 자료를 자동 삭제하지 않습니다.\n"
                      "기기 등록·수업 준비·학생 답안 작성은 연결했습니다. 실제 전송 수신·교사·학부모 기능과 정식 업데이트는 구현 중입니다.\n")
            with zipfile.ZipFile(portable, "x", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as z:
                z.write(executable, "thinkgrow-" + role + ".exe")
                z.writestr("README.txt", readme.encode("utf-8-sig"))
                z.write(ROOT / "installer/windows/LICENSE.Tauri-MIT", "licenses/Tauri-MIT.txt")
            for path, kind in ((setup, "installer"), (portable, "portable")):
                item = {"role": role, "app_id": config["identifier"], "platform": "windows-x86_64",
                        "version": version, "package_type": kind, "file": path.name,
                        "bytes": path.stat().st_size, "sha256": digest(path)}
                report["artifacts"].append(item)
                (output / (path.name + ".sha256")).write_text(item["sha256"] + "  " + path.name + "\n", encoding="ascii")
            print(role + ": installer + standalone ZIP built (unpublished)", flush=True)
        if any(digest(ROOT / path) != value for path,value in source_hashes.items()):
            raise RuntimeError("Source changed during packaging; rebuild before using these artifacts")
        report["status"] = "built_unpublished"
    except Exception:
        report["status"] = "failed"
        raise
    finally:
        (output / "build-manifest.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
