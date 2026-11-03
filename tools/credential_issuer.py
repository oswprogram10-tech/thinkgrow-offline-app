"""Windows-protected credential issuer. Never prints/exports private keys."""
import argparse
import base64
import ctypes
from ctypes import wintypes
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

ENTROPY = b"thinkgrow.offline.credential.issuer.v1"
ROOT = Path(__file__).resolve().parents[1]


class Blob(ctypes.Structure):
    _fields_ = [("size", wintypes.DWORD), ("data", ctypes.POINTER(ctypes.c_ubyte))]


def protected(raw, encrypt):
    if sys.platform != "win32": raise ValueError("Windows protected storage required")
    crypt = ctypes.WinDLL("crypt32", use_last_error=True)
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.LocalFree.argtypes = [ctypes.c_void_p]; kernel.LocalFree.restype = ctypes.c_void_p
    data = ctypes.create_string_buffer(raw); salt = ctypes.create_string_buffer(ENTROPY)
    source = Blob(len(raw),ctypes.cast(data,ctypes.POINTER(ctypes.c_ubyte)))
    entropy = Blob(len(ENTROPY),ctypes.cast(salt,ctypes.POINTER(ctypes.c_ubyte)))
    result = Blob()
    function = crypt.CryptProtectData if encrypt else crypt.CryptUnprotectData
    function.argtypes = [ctypes.POINTER(Blob),ctypes.c_void_p,ctypes.POINTER(Blob),ctypes.c_void_p,ctypes.c_void_p,wintypes.DWORD,ctypes.POINTER(Blob)]
    function.restype = wintypes.BOOL
    try:
        if not function(ctypes.byref(source),None,ctypes.byref(entropy),None,None,1,ctypes.byref(result)):
            raise ValueError("Windows protection operation failed")
        if result.size > 16384: raise ValueError("Invalid protected key length")
        return ctypes.string_at(result.data,result.size)
    finally:
        ctypes.memset(data,0,len(data))
        if result.data:
            if not encrypt: ctypes.memset(result.data,0,result.size)
            kernel.LocalFree(ctypes.cast(result.data,ctypes.c_void_p))


def public(record):
    seed = base64.b64decode(record["seed"],validate=True)
    if len(seed)!=32 or record.get("format")!=1 or not re.fullmatch(r"[A-Za-z0-9_-]{1,80}",record.get("key_id","")):
        raise ValueError("Invalid issuer record")
    key = Ed25519PrivateKey.from_private_bytes(seed)
    raw = key.public_key().public_bytes(serialization.Encoding.Raw,serialization.PublicFormat.Raw)
    return {"format":1,"purpose":"offline-credential","key_id":record["key_id"],
            "public_key":base64.b64encode(raw).decode("ascii"),"fingerprint":hashlib.sha256(raw).hexdigest()}


def load(path):
    if path.is_symlink() or not path.is_file() or path.stat().st_size>16384: raise ValueError("Invalid protected key file")
    record = json.loads(protected(path.read_bytes(),False)); public(record); return record


def create(path,key_id):
    if not re.fullmatch(r"[A-Za-z0-9_-]{1,80}",key_id): raise ValueError("Invalid key ID")
    if path.resolve().is_relative_to(ROOT): raise ValueError("Private key must remain outside the repository")
    path.parent.mkdir(parents=True,exist_ok=True)
    if path.parent.is_symlink(): raise ValueError("Linked key directory refused")
    key = Ed25519PrivateKey.generate()
    seed = key.private_bytes(serialization.Encoding.Raw,serialization.PrivateFormat.Raw,serialization.NoEncryption())
    record = {"format":1,"key_id":key_id,"seed":base64.b64encode(seed).decode("ascii"),"created_at":datetime.datetime.now(datetime.timezone.utc).isoformat()}
    sealed = protected(json.dumps(record).encode(),True)
    with path.open("xb") as f: f.write(sealed); f.flush(); os.fsync(f.fileno())
    reopened = load(path)
    if public(reopened)!=public(record): raise ValueError("Issuer recovery check failed")
    return public(record)


def pin(policy,metadata):
    document = json.loads(policy.read_text(encoding="utf-8"))
    keys = document["credential_keys"]
    if metadata["key_id"] in keys and keys[metadata["key_id"]]!=metadata["public_key"]: raise ValueError("Key ID collision")
    keys[metadata["key_id"]] = metadata["public_key"]
    # Key preparation does not turn on an incomplete enrollment service.
    document.setdefault("registration_enabled",False)
    temporary = policy.with_suffix(".json.new")
    with temporary.open("x",encoding="utf-8") as f:
        f.write(json.dumps(document,ensure_ascii=False,indent=2)+"\n");f.flush();os.fsync(f.fileno())
    os.replace(temporary,policy)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action",choices=("create","public","run"))
    parser.add_argument("--key-file",type=Path,required=True)
    parser.add_argument("--key-id")
    parser.add_argument("--pin-policy",type=Path)
    args,command=parser.parse_known_args()
    if args.action=="create":
        if not args.key_id or command: parser.error("create requires --key-id and no trailing command")
        metadata=create(args.key_file,args.key_id)
    else:
        record=load(args.key_file);metadata=public(record)
    if args.pin_policy: pin(args.pin_policy,metadata)
    if args.action=="run":
        if command and command[0]=="--":command=command[1:]
        if not command:parser.error("run requires a child command after --")
        environment=os.environ.copy()
        environment["THINKGROW_OFFLINE_CREDENTIAL_KEY_ID"]=record["key_id"]
        environment["THINKGROW_OFFLINE_CREDENTIAL_KEY"]=record["seed"]
        # Only the trusted child process receives this environment. Never print it.
        sys.exit(subprocess.run(command,env=environment,stdin=subprocess.DEVNULL).returncode)
    if command:parser.error("Unexpected trailing command")
    print(json.dumps(metadata))


if __name__=="__main__": main()
