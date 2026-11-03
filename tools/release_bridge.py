"""Build an immutable, signed offline release directory; no upload/deploy side effects."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
from urllib.parse import urlparse

ROOT=Path(__file__).resolve().parents[1]
EXPECTED="oswprogram10-tech/school-class-allinone-platform"

def run(args,cwd=None,env=None):
    p=subprocess.run([str(a) for a in args],cwd=cwd,env=env,capture_output=True,text=True,encoding="utf-8",errors="replace",stdin=subprocess.DEVNULL,timeout=120)
    if p.returncode: raise ValueError("Command failed: "+str(args[0]))  # Signing stdout may contain secrets.
    return p.stdout.strip()

def commit(repo,ref):
    if not re.fullmatch(r"[a-fA-F0-9]{40}",ref): raise ValueError("Full source commit required")
    found=run(["git","rev-parse","--verify",ref+"^{commit}"],repo)
    if found.lower()!=ref.lower():raise ValueError("Source commit mismatch")
    return found

def origin(repo):
    value=run(["git","remote","get-url","origin"],repo).removesuffix(".git")
    if value not in ("https://github.com/"+EXPECTED,"git@github.com:"+EXPECTED):
        raise ValueError("Online repository mismatch")

def plan(repo,base,head,config):
    origin(repo)
    base,head=commit(repo,base),commit(repo,head)
    if subprocess.run(["git","merge-base","--is-ancestor",base,head],cwd=repo,capture_output=True).returncode:
        raise ValueError("Baseline is not an ancestor; integration review required")
    p=subprocess.run(["git","diff","--name-only","-z",base,head],cwd=repo,capture_output=True,check=True)
    paths=p.stdout.decode("utf-8").strip("\0").split("\0") if p.stdout else []
    decisions=[]
    for path in paths:
        action=next((r["action"] for r in config["rules"] if path.startswith(r["prefix"])),config["default_action"])
        decisions.append({"path":path,"action":action})
    actions={d["action"] for d in decisions}
    return {"format":1,"online_repository":EXPECTED,"base_commit":base,"online_commit":head,
            "changes":decisions,"native_rebuild_required":bool(actions-{"documentation","game_module"}),
            "game_release_required":"game_module" in actions,
            "automatic_publish_allowed":False,
            "note":"Review/adapt shared and online-only changes, then build pinned offline sources and sign release artifacts."}

def slug(s):
    return isinstance(s,str) and re.fullmatch(r"[a-z0-9][a-z0-9-]{0,79}",s)
def version(s):
    return isinstance(s,str) and re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-[0-9A-Za-z.-]+)?",s)
def sha(path):
    h=hashlib.sha256()
    with path.open("rb") as f:
        for block in iter(lambda:f.read(65536),b""):h.update(block)
    return h.hexdigest()

def prepare(args):
    online,offline=Path(args.online_repo).resolve(),Path(args.offline_repo).resolve()
    origin(online)
    source=commit(online,args.online_commit)
    offline_commit=run(["git","rev-parse","HEAD"],offline)
    commit(offline,offline_commit)
    if run(["git","status","--porcelain"],offline):
        raise ValueError("Offline release source must be committed and clean")
    spec=json.loads(Path(args.spec).read_text(encoding="utf-8"))
    if spec["channel"] not in ("stable","beta") or not isinstance(spec["sequence"],int) or not 0<spec["sequence"]<2**63:
        raise ValueError("Channel or sequence invalid")
    if not (0<=spec["issued_ms"]<spec["expires_ms"]<=spec["issued_ms"]+31*86400000):
        raise ValueError("Catalog lifetime invalid")
    base=args.base_url.rstrip("/")
    uri=urlparse(base)
    if uri.scheme!="https" or not uri.netloc or uri.username or uri.password or uri.query or uri.fragment:
        raise ValueError("HTTPS release base URL required")
    output=Path(args.output).resolve()
    if output.exists():raise ValueError("Published release directories are immutable")
    output.parent.mkdir(parents=True,exist_ok=True)
    env=os.environ.copy()
    if args.password_env:
        if args.password_env not in env:raise ValueError("Signing password environment variable missing")
        env["TAURI_SIGNING_PRIVATE_KEY_PASSWORD"]=env[args.password_env]
    cli=ROOT/"node_modules/@tauri-apps/cli/tauri.js"
    def sign(file,app_version=None):
        command=["node",cli,"signer","sign","--private-key-path",Path(args.private_key).resolve()]
        if app_version:command+=["--app-version",app_version]
        run([*command,file],env=env)
        return Path(str(file)+".sig").read_text().strip()
    with tempfile.TemporaryDirectory(prefix="offline-release-",dir=output.parent) as temporary:
        staging=Path(temporary);artifacts=[];identities=set()
        for a in spec["artifacts"]:
            if not slug(a["component"]) or a["kind"] not in ("app","shared","game") or not version(a["version"]) or not version(a["min_core"]):
                raise ValueError("Artifact identity/version invalid")
            app_id=a["app_id"]; platform=a["platform"]
            if app_id not in ["com.thinkgrow.offline."+r for r in ("teacher","student","parent")]:
                raise ValueError("Artifact app ID invalid")
            if platform not in ("windows-x86_64","android-aarch64","ios-aarch64"):
                raise ValueError("Artifact platform invalid")
            if (app_id.endswith(".teacher") and platform!="windows-x86_64") or (app_id.endswith(".parent") and platform=="windows-x86_64"):
                raise ValueError("Unsupported app/platform")
            if spec["channel"]=="stable" and "-" in a["version"]:raise ValueError("Prerelease on stable channel")
            identity=(a["component"],app_id,platform)
            if identity in identities:raise ValueError("Duplicate artifact")
            identities.add(identity)
            for lower,upper in [("protocol_min","protocol_max"),("schema_min","schema_max")]:
                if not (0<a[lower]<=a[upper]<2**32):raise ValueError("Compatibility range invalid")
            source_path=Path(a["path"]).resolve()
            size=source_path.stat().st_size
            if not 0<size<=1024**3:raise ValueError("Artifact size invalid")
            digest=sha(source_path)
            suffix=source_path.suffix if re.fullmatch(r"\.[A-Za-z0-9]{1,8}",source_path.suffix) else ".bin"
            name=f'{a["component"]}-{app_id.rsplit(".",1)[-1]}-{platform}-{a["version"]}-{digest[:16]}{suffix}'
            target=staging/name
            shutil.copyfile(source_path,target)
            if sha(target)!=digest:raise ValueError("Artifact changed during copy")
            signature=sign(target,a["version"])
            artifacts.append({k:a[k] for k in ("component","kind","app_id","platform","version","min_core",
                              "protocol_min","protocol_max","schema_min","schema_max")}|
                             {"url":base+"/"+name,"bytes":size,"sha256":digest,"signature":signature})
        if not artifacts or len(artifacts)>256:raise ValueError("Artifact count invalid")
        catalog={"format":1,"channel":spec["channel"],"sequence":spec["sequence"],"issued_ms":spec["issued_ms"],
                 "expires_ms":spec["expires_ms"],"online_repository":EXPECTED,"online_commit":source,
                 "offline_commit":offline_commit,"artifacts":artifacts}
        file=staging/"catalog.json"
        file.write_text(json.dumps(catalog,ensure_ascii=False,indent=2)+"\n",encoding="utf-8")
        sign(file)
        # Rename within the same filesystem. TemporaryDirectory owns the now-empty parent.
        final=staging/"ready";final.mkdir()
        for entry in list(staging.iterdir()):
            if entry!=final:entry.rename(final/entry.name)
        final.rename(output)
    return {"output":str(output),"online_commit":source,"offline_commit":offline_commit,
            "artifacts":len(artifacts),"published":False}

def main():
    parser=argparse.ArgumentParser()
    sub=parser.add_subparsers(dest="command",required=True)
    p=sub.add_parser("plan")
    p.add_argument("--online-repo",required=True);p.add_argument("--base");p.add_argument("--head",required=True)
    p.add_argument("--output",required=True)
    r=sub.add_parser("prepare")
    for name in ("online-repo","online-commit","offline-repo","spec","output","base-url","private-key"):
        r.add_argument("--"+name,required=True)
    r.add_argument("--password-env")
    args=parser.parse_args()
    try:
        if args.command=="plan":
            config=json.loads((ROOT/"release/bridge.json").read_text(encoding="utf-8"))
            result=plan(Path(args.online_repo),args.base or config["baseline_commit"],args.head,config)
            Path(args.output).parent.mkdir(parents=True,exist_ok=True)
            Path(args.output).write_text(json.dumps(result,ensure_ascii=False,indent=2)+"\n",encoding="utf-8")
        else:result=prepare(args)
        print(json.dumps(result,ensure_ascii=False,indent=2))
    except (ValueError,KeyError,OSError,subprocess.SubprocessError) as e:
        parser.exit(1,str(e)+"\n")
if __name__=="__main__":main()
