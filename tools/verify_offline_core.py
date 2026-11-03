"""Reproduce core and release-bridge checks with synthetic data only."""
import datetime
import hashlib,json,os,re,shutil,subprocess,sys,time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
DATE=datetime.datetime.now(datetime.timezone(datetime.timedelta(hours=9))).date().isoformat()
def main():
 env=os.environ.copy();env["CARGO_PROFILE_DEV_DEBUG"]="0";env["CARGO_PROFILE_TEST_DEBUG"]="0";env["PYTHONUTF8"]="1"
 cargo=shutil.which("cargo") or str(Path.home()/".cargo/bin/cargo.exe")
 logdir=ROOT/"target/core-validation";logdir.mkdir(parents=True,exist_ok=True)
 commands=[("teacher-ui-build",[shutil.which("node") or "node","scripts/build-ui.mjs","teacher"]),
  ("student-ui-build",[shutil.which("node") or "node","scripts/build-ui.mjs","student"]),
  ("parent-ui-build",[shutil.which("node") or "node","scripts/build-ui.mjs","parent"]),
  ("core-tests",[cargo,"test","--locked","-p","thinkgrow-offline-core"]),
  ("native-boundary-check",[cargo,"check","--locked","-p","teacher","-p","student","-p","parent"]),
  ("release-verifier-build",[cargo,"build","--locked","-p","thinkgrow-offline-core","--bin","release-check"]),
  ("release-bridge-tests",[sys.executable,"tools/test_release_bridge.py"])]
 report={"date_kst":DATE,"status":"running","source_uncommitted":True,"production_deployment":False,"steps":[]}
 try:
  for name,args in commands:
   started=time.monotonic()
   p=subprocess.run(args,cwd=ROOT,env=env,capture_output=True,text=True,encoding="utf-8",errors="replace")
   output=p.stdout+"\n"+p.stderr
   (logdir/(name+".log")).write_text(output,encoding="utf-8")
   step={"name":name,"exit_code":p.returncode,"seconds":round(time.monotonic()-started,3),
         "log_sha256":hashlib.sha256(output.encode()).hexdigest()}
   if name=="core-tests":step["passed"]=sum(map(int,re.findall(r"test result: ok\. (\d+) passed",output)))
   report["steps"].append(step)
   print(name+": "+str(p.returncode),flush=True)
   if p.returncode:
    print(output[-5000:]);raise RuntimeError(name+" failed")
  bridge=json.loads((ROOT/f"docs/build-evidence/release-bridge-{DATE}.json").read_text(encoding="utf-8"))
  report["release_bridge_tests"]=bridge["tests_run"]
  files=[*ROOT.glob("crates/offline-core/src/**/*.rs"),*ROOT.glob("crates/offline-core/tests/*.rs"),
         ROOT/"crates/offline-core/src/schema.sql",ROOT/"tools/release_bridge.py",ROOT/"Cargo.lock"]
  report["source_sha256"]={str(p.relative_to(ROOT)).replace("\\","/"):hashlib.sha256(p.read_bytes()).hexdigest() for p in files}
  report["status"]="passed"
 except Exception as e:
  report["status"]="failed";report["failure"]=str(e);raise
 finally:
  (ROOT/f"docs/build-evidence/offline-core-{DATE}.json").write_text(json.dumps(report,indent=2)+"\n",encoding="utf-8")
if __name__=="__main__":main()
