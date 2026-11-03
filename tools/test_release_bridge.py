import datetime
import importlib.util
import json
import os
from pathlib import Path
import secrets
import subprocess
import tempfile
import unittest

ROOT=Path(__file__).resolve().parents[1]
DATE=datetime.datetime.now(datetime.timezone(datetime.timedelta(hours=9))).date().isoformat()
spec=importlib.util.spec_from_file_location("bridge",ROOT/"tools/release_bridge.py")
bridge=importlib.util.module_from_spec(spec);spec.loader.exec_module(bridge)
BIN=ROOT/"target/debug/release-check.exe"
class ReleaseBridgeTests(unittest.TestCase):
 @classmethod
 def setUpClass(cls):
  cls.tmp=tempfile.TemporaryDirectory(prefix="release-bridge-",dir=ROOT/"target")
  cls.home=Path(cls.tmp.name);cls.online=cls.home/"online";cls.offline=cls.home/"offline"
  def git(repo,*args):
   return subprocess.check_output(["git","-C",str(repo),*args],text=True,encoding="utf-8").strip()
  def commit(repo,message):
   git(repo,"add",".");git(repo,"-c","user.name=Fixture","-c","user.email=fixture@users.noreply.github.com","commit","-qm",message)
   return git(repo,"rev-parse","HEAD")
  for repo in (cls.online,cls.offline):
   repo.mkdir();git(repo,"init","-q");(repo/"README.md").write_text("synthetic source");commit(repo,"base")
  git(cls.online,"remote","add","origin","https://github.com/"+bridge.EXPECTED)
  cls.base=git(cls.online,"rev-parse","HEAD")
  (cls.online/"cloud").mkdir();(cls.online/"cloud/question_types.py").write_text("# synthetic pure rules")
  (cls.online/"unknown").mkdir();(cls.online/"unknown/code.py").write_text("# manual review required")
  (cls.online/"게임_서버").mkdir();(cls.online/"게임_서버/game.js").write_text("// game change")
  cls.head=commit(cls.online,"synthetic changes")
  cls.artifact=cls.home/"teacher-fixture.zip";cls.artifact.write_bytes(b"synthetic signed update artifact")
  cls.env=os.environ.copy();cls.env["THINKGROW_FIXTURE_PASSWORD"]=secrets.token_hex(24)
  cls.env["TAURI_SIGNING_PRIVATE_KEY_PASSWORD"]=cls.env["THINKGROW_FIXTURE_PASSWORD"]
  cls.key=cls.home/"fixture.key";cls.other=cls.home/"other.key"
  for key in (cls.key,cls.other):
   p=subprocess.run(["node",str(ROOT/"node_modules/@tauri-apps/cli/tauri.js"),"signer","generate","--ci",
    "--password",cls.env["THINKGROW_FIXTURE_PASSWORD"],"--write-keys",str(key)],capture_output=True,env=cls.env)
   if p.returncode:raise AssertionError("Fixture key generation failed")
  cls.counter=0
  cls.release=cls.prepare()
 @classmethod
 def tearDownClass(cls):cls.tmp.cleanup()
 @classmethod
 def prepare(cls,sequence=2,**overrides):
  cls.counter+=1
  a={"path":str(cls.artifact),"component":"offline-app","kind":"app","app_id":"com.thinkgrow.offline.teacher",
   "platform":"windows-x86_64","version":"0.2.0","min_core":"0.1.0","protocol_min":1,"protocol_max":1,"schema_min":1,"schema_max":1}
  a.update(overrides)
  spec=cls.home/f"spec-{cls.counter}.json"
  spec.write_text(json.dumps({"channel":"stable","sequence":sequence,"issued_ms":1000,"expires_ms":100000,
    "artifacts":[a]}))
  output=cls.home/f"release-{cls.counter}"
  p=subprocess.run([os.sys.executable,str(ROOT/"tools/release_bridge.py"),"prepare",
    "--online-repo",str(cls.online),"--online-commit",cls.head,"--offline-repo",str(cls.offline),
    "--spec",str(spec),"--output",str(output),"--base-url","https://updates.example.invalid/releases/fixture",
    "--private-key",str(cls.key),"--password-env","THINKGROW_FIXTURE_PASSWORD"],
    capture_output=True,text=True,encoding="utf-8",env=cls.env)
  if p.returncode:raise AssertionError("Prepare failed: "+p.stderr)
  return output
 def invoke(self,release=None,*,role="teacher",platform="windows-x86_64",channel="stable",now=2000,
            current="0.1.0",key=None,ledger=None,policy=None,artifact=None,ok=True):
  release=release or self.release
  catalog=json.loads((release/"catalog.json").read_text(encoding="utf-8"))
  file=artifact or next(f for f in release.iterdir() if f.name not in ("catalog.json","catalog.json.sig") and not f.name.endswith(".sig"))
  ledger=ledger or self.home/("ledger-"+secrets.token_hex(8))
  args=[str(BIN),str(release/"catalog.json"),str(release/"catalog.json.sig"),str(key or Path(str(self.key)+".pub")),
   str(ledger),role,platform,channel,str(now),"offline-app",current,"0.1.0",str(file)]
  if policy:args.append(policy)
  result=subprocess.run(args,capture_output=True,text=True,encoding="utf-8",errors="replace")
  self.assertEqual(result.returncode==0,ok,result.stderr)
  return result
 def test_signed_catalog_and_artifact_stage_in_native_core(self):
  result=json.loads(self.invoke().stdout);self.assertEqual(result["route"],"tauri-desktop-updater")
  self.assertTrue(Path(result["path"]).exists())
 def test_tampered_catalog_rejected(self):
  target=self.home/"tampered-catalog";target.mkdir()
  for f in self.release.iterdir():
   if f.is_file(): (target/f.name).write_bytes(f.read_bytes())
  with (target/"catalog.json").open("ab") as f:f.write(b" ")
  self.invoke(target,ok=False)
 def test_tampered_artifact_rejected(self):
  file=self.home/"tampered.zip";file.write_bytes(b"tampered")
  self.invoke(artifact=file,ok=False)
 def test_wrong_pinned_key_rejected(self):self.invoke(key=Path(str(self.other)+".pub"),ok=False)
 def test_wrong_role_and_platform_rejected(self):
  self.invoke(role="student",ok=False);self.invoke(role="parent",ok=False)
 def test_expired_feed_and_wrong_channel_rejected(self):
  self.invoke(now=100000,ok=False);self.invoke(channel="beta",ok=False)
 def test_downgrade_and_replay_rejected(self):
  self.invoke(current="0.3.0",ok=False);self.invoke(current="0.2.0",ok=False)
 def test_sequence_rollback_rejected(self):
  ledger=self.home/"rollback-ledger";self.invoke(ledger=ledger)
  old=self.prepare(sequence=1);self.invoke(old,ledger=ledger,ok=False)
 def test_incompatible_protocol_and_schema_rejected(self):
  for key in ("protocol_min","schema_min"):
   release=self.prepare(**{key:2,key.replace("min","max"):2})
   self.invoke(release,ok=False)
 def test_class_and_consent_policy_enforced(self):
  self.invoke(policy="in-class",ok=False);self.invoke(policy="no-consent",ok=False)
 def test_online_plan_classifies_real_git_changes(self):
  config=json.loads((ROOT/"release/bridge.json").read_text(encoding="utf-8"))
  plan=bridge.plan(self.online,self.base,self.head,config)
  self.assertTrue(plan["native_rebuild_required"]);self.assertTrue(plan["game_release_required"])
  self.assertFalse(plan["automatic_publish_allowed"])
  self.assertIn("manual_review",{d["action"] for d in plan["changes"]})
 def test_unrelated_repository_is_rejected(self):
  config=json.loads((ROOT/"release/bridge.json").read_text(encoding="utf-8"))
  with self.assertRaises(ValueError):bridge.plan(self.offline,self.base,self.head,config)

if __name__=="__main__":
 result=unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(ReleaseBridgeTests))
 (ROOT/f"docs/build-evidence/release-bridge-{DATE}.json").write_text(json.dumps({
  "tests_run":result.testsRun,"failures":len(result.failures),"errors":len(result.errors),
  "status":"passed" if result.wasSuccessful() else "failed","published":False,
  "fixtures":"temporary synthetic Git repositories, artifacts and Tauri signing keys"
 },indent=2)+"\n")
 raise SystemExit(0 if result.wasSuccessful() else 1)
