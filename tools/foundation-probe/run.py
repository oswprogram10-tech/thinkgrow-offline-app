"""Host-only fault-injection experiments; never use this harness as an app updater."""
import base64
import concurrent.futures
import contextlib
import datetime as dt
import hashlib
import json
import logging
import os
from pathlib import Path
import re
import secrets
import shutil
import sqlite3
import ssl
import subprocess
import sys
import tempfile
import threading
import time
import uuid

ROOT = Path(__file__).resolve().parents[2]
EXE = ROOT / "target/foundation-probe/debug/thinkgrow-foundation-probe.exe"
TAURI = ROOT / "node_modules/@tauri-apps/cli/tauri.js"
CHECKS = []

def check(name, fn):
    started = time.monotonic()
    details = fn()
    CHECKS.append(dict(name=name, status="passed", seconds=round(time.monotonic()-started, 3), details=details))
    print("PASS " + name, flush=True)
    return details

def require(value, message):
    if not value:
        raise AssertionError(message)

def command(args, data=None, ok=True):
    p = subprocess.run([str(x) for x in args], input=data, text=True, encoding="utf-8", errors="replace", capture_output=True, timeout=60)
    if ok:
        require(p.returncode == 0, "Command failed: " + str(args[1]) + ": " + p.stderr[-700:])
    else:
        require(p.returncode != 0, "Negative case unexpectedly accepted")
    return p

def probe(*args, data=None, ok=True):
    return command([EXE, *args], data, ok)

def read_db(path):
    return json.loads(probe("db-check", path).stdout)

def kill_at_ready(args, ready):
    p = subprocess.Popen([str(x) for x in args], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, encoding="utf-8", errors="replace")
    try:
        deadline = time.monotonic() + 15
        while not ready.exists():
            if p.poll() is not None:
                raise AssertionError("Worker exited before fault point: " + p.communicate()[1][-500:])
            if time.monotonic() > deadline:
                raise TimeoutError("Worker readiness timeout")
            time.sleep(.02)
        p.kill()  # Windows TerminateProcess: no destructor / graceful SQLite close.
        p.wait(timeout=10)
    finally:
        if p.poll() is None:
            p.kill()
            p.wait()
        if p.stdout: p.stdout.close()
        if p.stderr: p.stderr.close()

def sqlite_checks(work):
    version = None
    for phase in ("before-commit", "after-commit"):
        for attempt in range(3):
            d = work / f"sqlite-{phase}-{attempt}"
            d.mkdir()
            db = d / "data.db"
            seeded = json.loads(probe("db-seed", db).stdout)
            version = seeded["sqlite"]
            require(tuple(map(int, version.split("."))) >= (3, 51, 3), "SQLite WAL-reset fix required")
            ready = d / "ready"
            def crash():
                kill_at_ready([EXE, "db-worker", db, phase, ready], ready)
                actual = read_db(db)
                committed = phase == "after-commit"
                require(actual == dict(integrity="ok", draft="changed-in-transaction" if committed else "preserve-me",
                                       outbox=2 if committed else 1, schema=1, migration_column=0), "Crash recovery changed logical data")
                return actual
            check(f"sqlite.{phase}.{attempt+1}", crash)
    db = work / "sqlite-after-commit-2/data.db"
    def migration():
        previous = read_db(db)
        failure = probe("db-failed-migration", db, ok=False)
        require("missing_table" in failure.stderr, "Unexpected migration failure")
        require(read_db(db) == previous, "Failed migration was not rolled back")
        return previous
    check("sqlite.failed-migration-rollback", migration)
    backup = work / "backup.db"
    probe("db-backup", db, backup)
    def restore():
        damaged = work / "damaged.db"
        shutil.copy2(backup, damaged)
        with damaged.open("r+b") as f:
            f.write(b"deliberately-corrupt-database-header")
        probe("db-check", damaged, ok=False)
        restored = work / "restored.db"
        shutil.copy2(backup, restored)
        require(read_db(restored) == read_db(db), "Native backup did not preserve pending data")
        return read_db(restored)
    check("sqlite.corruption-detected-backup-restored", restore)
    return version, db

def credentials(run_id):
    services = {r: f"com.thinkgrow.offline.{r}.foundation-probe.{run_id}" for r in ("teacher", "student", "parent")}
    secret = secrets.token_hex(32)
    digest = hashlib.sha256(secret.encode()).hexdigest()
    try:
        for role, service in services.items():
            probe("credential", "digest", service, ok=False)
            if role != "teacher":
                probe("credential", "set", service, data=secrets.token_hex(32))
        probe("credential", "set", services["teacher"], data=secret)
        def persisted():
            require(probe("credential", "digest", services["teacher"]).stdout.strip() == digest, "Cross-process credential read failed")
            for role in ("student", "parent"):
                require(probe("credential", "digest", services[role]).stdout.strip() != digest, "Role namespace collision")
            return {"backend": "Windows Credential Manager", "cross_process": True, "namespaces": 3}
        check("credentials.persist-and-namespace", persisted)
        def rotate():
            replacement = secrets.token_hex(32)
            probe("credential", "set", services["teacher"], data=replacement)
            actual = probe("credential", "digest", services["teacher"]).stdout.strip()
            require(actual == hashlib.sha256(replacement.encode()).hexdigest() and actual != digest, "Credential rotation failed")
            return {"rotated": True, "plaintext_logged": False}
        check("credentials.rotate", rotate)
    finally:
        failures = []
        for service in services.values():
            try:
                probe("credential", "delete", service)
            except Exception:
                failures.append(service)
        require(not failures, "Probe credential cleanup failed")
    def deleted():
        for service in services.values():
            probe("credential", "digest", service, ok=False)
        return {"synthetic_credentials_removed": 3}
    check("credentials.delete", deleted)

def certificates(work):
    from cryptography import x509
    from cryptography.hazmat.primitives import hashes, serialization
    from cryptography.hazmat.primitives.asymmetric import rsa
    from cryptography.x509.oid import NameOID, ExtendedKeyUsageOID
    now = dt.datetime.now(dt.timezone.utc)
    def authority(name):
        key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
        subject = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, name)])
        cert = (x509.CertificateBuilder().subject_name(subject).issuer_name(subject).public_key(key.public_key())
                .serial_number(x509.random_serial_number()).not_valid_before(now-dt.timedelta(days=1))
                .not_valid_after(now+dt.timedelta(days=2)).add_extension(x509.BasicConstraints(ca=True, path_length=0), True)
                .add_extension(x509.KeyUsage(False, False, False, False, False, True, True, False, False), True)
                .sign(key, hashes.SHA256()))
        path = work / (name + ".pem")
        path.write_bytes(cert.public_bytes(serialization.Encoding.PEM))
        return key, cert, path
    ca_key, ca, ca_path = authority("thinkgrow-probe-ca")
    _, _, rogue = authority("thinkgrow-probe-other-ca")
    def leaf(expired):
        key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
        name = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, "localhost")])
        cert = (x509.CertificateBuilder().subject_name(name).issuer_name(ca.subject).public_key(key.public_key())
                .serial_number(x509.random_serial_number())
                .not_valid_before(now-dt.timedelta(days=3) if expired else now-dt.timedelta(minutes=5))
                .not_valid_after(now-dt.timedelta(days=1) if expired else now+dt.timedelta(days=1))
                .add_extension(x509.SubjectAlternativeName([x509.DNSName("localhost")]), False)
                .add_extension(x509.BasicConstraints(ca=False, path_length=None), True)
                .add_extension(x509.ExtendedKeyUsage([ExtendedKeyUsageOID.SERVER_AUTH]), False)
                .sign(ca_key, hashes.SHA256()))
        prefix = "expired" if expired else "valid"
        crt, pem = work / (prefix+".crt"), work / (prefix+".key")
        crt.write_bytes(cert.public_bytes(serialization.Encoding.PEM))
        pem.write_bytes(key.private_bytes(serialization.Encoding.PEM, serialization.PrivateFormat.PKCS8, serialization.NoEncryption()))
        ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        ctx.minimum_version = ssl.TLSVersion.TLSv1_2
        ctx.load_cert_chain(crt, pem)
        return ctx
    return ca_path, rogue, leaf(False), leaf(True)

@contextlib.contextmanager
def server(context, work, token):
    from websockets.sync.server import serve
    db = sqlite3.connect(work / "receipts.db", check_same_thread=False)
    db.execute("CREATE TABLE IF NOT EXISTS received(id TEXT PRIMARY KEY, payload TEXT, receipt TEXT)")
    db.commit()
    lock, dropped = threading.Lock(), set()
    def handler(ws):
        data = json.loads(ws.recv(timeout=5))
        if data.get("token") != token or data.get("role") != "student":
            ws.close(1008, "probe authorization denied")
            return
        with lock:
            row = db.execute("SELECT payload,receipt FROM received WHERE id=?", (data["id"],)).fetchone()
            if row and row[0] != data["payload"]:
                ws.close(1008, "operation payload conflict")
                return
            receipt = row[1] if row else "receipt-" + data["id"]
            if not row:
                db.execute("INSERT INTO received VALUES(?,?,?)", (data["id"], data["payload"], receipt))
                db.commit()  # Durable receive precedes acknowledgement.
            drop = data.get("drop_ack") and data["id"] not in dropped
            if drop: dropped.add(data["id"])
        if drop:
            ws.close(1011, "synthetic acknowledgement loss")
        else:
            ws.send(json.dumps({"id": data["id"], "receipt": receipt}))
    logger = logging.getLogger("foundation-wss")
    logger.disabled = True
    try:
        with serve(handler, "127.0.0.1", 0, ssl=context, compression=None, max_size=65536, logger=logger) as srv:
            thread = threading.Thread(target=srv.serve_forever, daemon=True)
            thread.start()
            try: yield srv.socket.getsockname()[1]
            finally:
                srv.shutdown()
                thread.join(timeout=5)
    finally:
        db.close()

def transport(work):
    ca, rogue, good_ctx, expired_ctx = certificates(work)
    token = secrets.token_hex(32)
    with server(good_ctx, work, token) as port:
        def request(op, *, root=ca, host="localhost", ok=True, **extra):
            payload = dict(token=token, role="student", id=op, payload="synthetic-answer")
            payload.update(extra)
            result = probe("ws", root, str(port), host, data=json.dumps(payload), ok=ok)
            return json.loads(result.stdout) if ok else {"rejected": True, "error": result.stderr.strip()[:220]}
        check("tls.trusted-ca-and-hostname", lambda: request("baseline"))
        check("tls.untrusted-ca-rejected", lambda: request("bad-ca", root=rogue, ok=False))
        check("tls.hostname-mismatch-rejected", lambda: request("bad-host", host="wrong.invalid", ok=False))
        check("ws.invalid-token-rejected", lambda: request("bad-token", token="invalid", ok=False))
        check("ws.wrong-role-rejected", lambda: request("bad-role", role="parent", ok=False))
        def replay():
            lost = request("ack-loss", drop_ack=True, ok=False)
            a, b = request("ack-loss"), request("ack-loss")
            require(a == b, "Replay produced different receipt")
            return {"lost_ack_rejected": lost["rejected"], "receipt_reused": True}
        check("ws.disconnect-reconnect-deduplicate", replay)
        check("ws.same-id-different-payload-rejected", lambda: request("ack-loss", payload="tampered", ok=False))
        def concurrent_clients():
            barrier = threading.Barrier(30)
            def client(i):
                barrier.wait(timeout=15)
                return request("student-" + str(i))
            with concurrent.futures.ThreadPoolExecutor(max_workers=30) as pool:
                results = list(pool.map(client, range(30)))
            require(len({r["receipt"] for r in results}) == 30, "Concurrent receipt loss or collision")
            return {"virtual_clients": 30, "unique_receipts": 30, "loopback_only": True}
        check("ws.thirty-virtual-clients", concurrent_clients)
    with server(expired_ctx, work, token) as port:
        def expired_rejected():
            valid_request = dict(token=token, role="student", id="expired-cert", payload="synthetic-answer")
            rejected = probe("ws", ca, str(port), "localhost", data=json.dumps(valid_request), ok=False)
            return {"rejected": True, "error": rejected.stderr.strip()[:220]}
        check("tls.expired-certificate-rejected", expired_rejected)
    with contextlib.closing(sqlite3.connect(work / "receipts.db")) as db:
        require(db.execute("SELECT count(*) FROM received").fetchone()[0] == 32, "Unauthorized write or replay duplication")
    return {"listener": "127.0.0.1 only", "client_tls": "Rust native-tls / Windows SChannel", "server": "Python ssl test fixture"}

def durable_json(path, data):
    staging = path.with_suffix(".next")
    with staging.open("w", encoding="utf-8") as f:
        json.dump(data, f, sort_keys=True)
        f.flush()
        os.fsync(f.fileno())
    os.replace(staging, path)

def recover_update(home):
    journal = home / "pending.json"
    if journal.exists():
        pending = json.loads(journal.read_text())
        durable_json(home / "active.json", {"version": pending["previous"]})
        journal.unlink()

def update_worker(home, phase, ready):
    previous = json.loads((home / "active.json").read_text())["version"]
    durable_json(home / "pending.json", {"previous": previous, "candidate": "2.0.0"})
    if phase == "after-switch":
        durable_json(home / "active.json", {"version": "2.0.0"})
    ready.write_text("ready")
    while True: time.sleep(1)

def updates(work, db):
    home = work / "updates"
    home.mkdir()
    (home / "versions").mkdir()
    durable_json(home / "active.json", {"version": "1.0.0"})
    artifact = home / "candidate.json"
    artifact.write_text(json.dumps({"app_id": "com.thinkgrow.offline.teacher", "version": "2.0.0",
                                    "payload": "synthetic executable placeholder"}, sort_keys=True))
    password = secrets.token_hex(24)
    keys = []
    env = os.environ.copy()
    env["TAURI_SIGNING_PRIVATE_KEY_PASSWORD"] = password
    def cli(args):
        p = subprocess.run(["node", str(TAURI), *args], capture_output=True, text=True, env=env, timeout=60)
        require(p.returncode == 0, "Tauri fixture signing failed")  # Never print generated private keys.
    for name in ("trusted", "wrong"):
        key = home / (name + ".key")
        cli(["signer", "generate", "--ci", "--password", password, "--write-keys", str(key)])
        raw_pub = home / (name + ".raw.pub")
        raw_pub.write_bytes(base64.b64decode(Path(str(key)+".pub").read_text().strip()))
        keys.append((key, raw_pub))
    cli(["signer", "sign", "--private-key-path", str(keys[0][0]), "--app-version", "2.0.0", str(artifact)])
    raw_sig = home / "candidate.raw.sig"
    raw_sig.write_bytes(base64.b64decode(Path(str(artifact)+".sig").read_text().strip()))
    pub = keys[0][1]
    before = db.read_bytes()
    check("update.tauri-signature-compatible", lambda: {"verified": probe("verify", pub, raw_sig, artifact).returncode == 0})
    modified = home / "modified.json"
    modified.write_bytes(artifact.read_bytes() + b" ")
    check("update.payload-tamper-rejected", lambda: {"rejected": probe("verify", pub, raw_sig, modified, ok=False).returncode != 0})
    check("update.wrong-key-rejected", lambda: {"rejected": probe("verify", keys[1][1], raw_sig, artifact, ok=False).returncode != 0})
    def policy(file, role, current, in_class, consent):
        probe("verify", pub, raw_sig, file)
        data = json.loads(file.read_text())
        require(data["app_id"] == role, "Role mismatch")
        require(re.fullmatch(r"\d+\.\d+\.\d+", data["version"]), "Invalid version")
        require(tuple(map(int, data["version"].split("."))) > tuple(map(int, current.split("."))), "Downgrade or replay")
        require(not in_class and consent, "Update requires consent outside class")
        return data
    for name, role, current, in_class, consent in [
        ("wrong-role", "com.thinkgrow.offline.student", "1.0.0", False, True),
        ("downgrade", "com.thinkgrow.offline.teacher", "3.0.0", False, True),
        ("replay", "com.thinkgrow.offline.teacher", "2.0.0", False, True),
        ("during-class", "com.thinkgrow.offline.teacher", "1.0.0", True, True),
        ("without-consent", "com.thinkgrow.offline.teacher", "1.0.0", False, False)]:
        def rejected(role=role, current=current, in_class=in_class, consent=consent):
            try: policy(artifact, role, current, in_class, consent)
            except AssertionError:
                unchanged = json.loads((home/"active.json").read_text())["version"] == "1.0.0"
                require(unchanged, "Rejected update changed active version")
                return {"rejected": True, "active_unchanged": True}
            raise AssertionError("Update policy unexpectedly accepted")
        check("update." + name + "-blocked", rejected)
    data = policy(artifact, "com.thinkgrow.offline.teacher", "1.0.0", False, True)
    candidate = home / "versions" / data["version"]
    candidate.mkdir()
    shutil.copy2(artifact, candidate / "artifact.json")
    for phase in ("before-switch", "after-switch"):
        def interruption(phase=phase):
            ready = home / ("ready-" + phase)
            kill_at_ready([sys.executable, Path(__file__), "update-worker", home, phase, ready], ready)
            recover_update(home)
            require(json.loads((home/"active.json").read_text())["version"] == "1.0.0", "Interrupted update failed rollback")
            require(db.read_bytes() == before, "Update touched user data")
            return {"restored": "1.0.0", "data_preserved": True}
        check("update.crash-" + phase, interruption)
    def failed_health():
        durable_json(home / "pending.json", {"previous": "1.0.0", "candidate": "2.0.0"})
        durable_json(home / "active.json", {"version": "2.0.0"})
        recover_update(home)
        require(json.loads((home/"active.json").read_text())["version"] == "1.0.0", "Health failure did not roll back")
        require(db.read_bytes() == before, "Failed health rollback touched user data")
        return {"restored": "1.0.0", "data_preserved": True}
    check("update.failed-health-rollback", failed_health)
    def success():
        durable_json(home / "pending.json", {"previous": "1.0.0", "candidate": "2.0.0"})
        durable_json(home / "active.json", {"version": "2.0.0"})
        (home / "pending.json").unlink()  # Fixture health acknowledgement.
        recover_update(home)
        require(json.loads((home/"active.json").read_text())["version"] == "2.0.0", "Healthy activation lost")
        require(db.read_bytes() == before, "Healthy activation touched data")
        return {"active": "2.0.0", "data_preserved": True, "installer_model_only": True}
    check("update.healthy-activation-model", success)
    # Entire work tree, including test signing keys and certificates, is removed by TemporaryDirectory.

def main():
    if len(sys.argv) > 1 and sys.argv[1] == "update-worker":
        update_worker(Path(sys.argv[2]), sys.argv[3], Path(sys.argv[4]))
        return
    if os.name != "nt": raise SystemExit("This probe requires Windows")
    run_id = uuid.uuid4().hex
    start = dt.datetime.now(dt.timezone.utc).isoformat()
    runs = ROOT / "target/foundation-runs"
    runs.mkdir(parents=True, exist_ok=True)
    report = {"started_utc": start, "run_id": run_id, "scope": "Windows host native probes and updater transaction model",
              "physical_devices": False, "school_lan": False, "production_deployment": False, "checks": CHECKS}
    try:
        with tempfile.TemporaryDirectory(prefix=run_id+"-", dir=runs) as temp:
            work = Path(temp)
            sqlite_version, db = sqlite_checks(work)
            report["sqlite_version"] = sqlite_version
            credentials(run_id)
            report["transport"] = transport(work)
            updates(work, db)
        report["status"] = "passed"
    except Exception as e:
        report["status"] = "failed"
        report["failure"] = str(e)
        raise
    finally:
        report["finished_utc"] = dt.datetime.now(dt.timezone.utc).isoformat()
        report["passed_checks"] = len(CHECKS)
        output = ROOT / "docs/build-evidence/foundation-2026-11-03.json"
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
        print(f"Evidence: {output}", flush=True)
    print(f"All {len(CHECKS)} checks passed", flush=True)

if __name__ == "__main__":
    main()