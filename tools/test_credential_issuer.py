import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
import credential_issuer as issuer

class IssuerTests(unittest.TestCase):
    def test_protected_roundtrip_and_public_policy_do_not_activate_service(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);key=root/'issuer.dpapi';policy=root/'policy.json'
            policy.write_text(json.dumps({"credential_keys":{"prior":"preserved"}}),encoding="utf-8")
            public=issuer.create(key,"synthetic-credential")
            self.assertNotIn(b'"seed"',key.read_bytes())
            self.assertEqual(public,issuer.public(issuer.load(key)))
            issuer.pin(policy,public);data=json.loads(policy.read_text(encoding="utf-8"))
            self.assertFalse(data['registration_enabled']);self.assertEqual(data['credential_keys']['prior'],'preserved')
            self.assertEqual(data['credential_keys'][public['key_id']],public['public_key'])
            with self.assertRaises(FileExistsError): issuer.create(key,"synthetic-credential")
    def test_tampered_protected_key_is_not_replaced(self):
        with tempfile.TemporaryDirectory() as temp:
            key=Path(temp)/'issuer.dpapi';issuer.create(key,"synthetic-credential")
            raw=bytearray(key.read_bytes());raw[-1]^=1;key.write_bytes(raw)
            with self.assertRaises(ValueError): issuer.load(key)
            self.assertEqual(key.read_bytes(),raw)
    def test_collision_and_repository_private_key_are_refused(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);key=root/'issuer.dpapi';policy=root/'policy.json';public=issuer.create(key,"synthetic-credential")
            policy.write_text(json.dumps({"credential_keys":{"synthetic-credential":"different"}}),encoding="utf-8")
            with self.assertRaises(ValueError):issuer.pin(policy,public)
            self.assertEqual(json.loads(policy.read_text(encoding="utf-8"))['credential_keys']['synthetic-credential'],'different')
            with self.assertRaises(ValueError):issuer.create(issuer.ROOT/'must-not-be-created.dpapi',"synthetic")

if __name__=='__main__':unittest.main()
