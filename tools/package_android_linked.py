"""Finish a successful Tauri Rust build using verified Windows hardlinks.

This does not change developer mode or elevation policy. The role-specific
native library must already have been built by the normal Tauri command.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import zipfile

ROOT=Path(__file__).resolve().parents[1]


def digest(path):
    with path.open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('role',choices=('student','parent'));args=parser.parse_args();role=args.role
    project=ROOT/f'apps/{role}/src-tauri/gen/android'
    filename=f'libthinkgrow_offline_{role}_lib.so'
    source=(ROOT/f'target/android/{role}/aarch64-linux-android/debug/{filename}').resolve()
    destination=project/f'app/src/main/jniLibs/arm64-v8a/{filename}'
    if not source.is_relative_to(ROOT/'target/android'/role) or not destination.absolute().is_relative_to(project):
        raise RuntimeError('Unexpected generated path')
    if source.is_symlink() or not source.is_file():raise RuntimeError('Build the role-specific Rust library first')
    settings=(project/'tauri.settings.gradle').read_text(encoding='utf-8')
    if 'tauri-plugin-thinkgrow-vault' not in settings:raise RuntimeError('Run Tauri Android build to prepare native plugin settings')
    before=digest(source);destination.parent.mkdir(parents=True,exist_ok=True)
    # Only this exact generated role-library directory entry is replaced.
    if destination.exists() or destination.is_symlink():destination.unlink()
    os.link(source,destination)
    if not os.path.samefile(source,destination) or digest(destination)!=before:raise RuntimeError('Native hardlink mismatch')
    env=os.environ.copy()
    for key in ('JAVA_HOME','ANDROID_HOME','NDK_HOME'):
        if not env.get(key):raise RuntimeError('Build environment missing: '+key)
    command=[str(project/'gradlew.bat'),':app:assembleUniversalDebug',':tauri-plugin-thinkgrow-vault:testDebugUnitTest',
             '-PabiList=arm64-v8a','-ParchList=arm64','-PtargetList=aarch64','-x',':app:rustBuildArm64Debug','--console=plain']
    result=subprocess.run(command,cwd=project,env=env)
    if result.returncode:raise RuntimeError('Gradle package or native cipher tests failed')
    apk=project/'app/build/outputs/apk/universal/debug/app-universal-debug.apk'
    with zipfile.ZipFile(apk) as archive:
        libraries=[name for name in archive.namelist() if name.startswith('lib/') and name.endswith('.so')]
        if libraries!=['lib/arm64-v8a/'+filename] or hashlib.sha256(archive.read(libraries[0])).hexdigest()!=before:
            raise RuntimeError('APK must contain the exact role-specific native library')
    if digest(source)!=before:raise RuntimeError('Native build changed while packaging')
    report={'role':role,'platform':'android-aarch64','profile':'debug','status':'built_unpublished',
            'apk':str(apk.relative_to(ROOT)).replace(chr(92),'/'),'sha256':digest(apk),'bytes':apk.stat().st_size,
            'native_sha256':before,'native_link_mode':'hardlink','device_executed':False,'keystore_device_test':False}
    evidence=ROOT/f'docs/build-evidence/mobile-vault-{role}-2026-11-03.json'
    evidence.write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8');print(json.dumps(report))

if __name__=='__main__':main()
