use crate::{crypto::DataKey,vault::Secrets,App,Error,Result};
use argon2::Argon2;
use rand::{rngs::OsRng,RngCore};
use serde::{Deserialize,Serialize};
use zeroize::Zeroizing;

const MAGIC:&[u8;8]=b"TGBAK1\0\0";
const MAX_CLEAR:usize=64*1024*1024;
pub(crate) const MAX_BACKUP:usize=96*1024*1024;
pub(crate) const MAX_SOURCE:usize=32*1024*1024;
pub(crate) const MAX_FILES:usize=4096;

#[derive(Serialize)]
pub(crate) struct BackupExport<'a> {
    pub format:u32,
    pub app_id:String,
    pub handle:String,
    pub label:String,
    pub enrolled:bool,
    pub wall_floor:i64,
    pub secrets:&'a Secrets,
    pub files:&'a [BackupFile],
}

#[derive(Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BackupPayload {
    pub format:u32,
    pub app_id:String,
    pub handle:String,
    pub label:String,
    pub enrolled:bool,
    pub wall_floor:i64,
    pub secrets:Secrets,
    pub files:Vec<BackupFile>,
}

#[derive(Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BackupFile {pub path:String,pub content:String}

pub(crate) fn encode<T:Serialize>(app:App,passphrase:&str,payload:&T)->Result<Vec<u8>> {
    let salt=random_salt();let key=derive(passphrase,&salt)?;
    let clear=Zeroizing::new(serde_json::to_vec(payload)?);
    if clear.len()>MAX_CLEAR {return Err(Error::Invalid("backup exceeds size limit"))}
    let encrypted=key.seal(&format!("thinkgrow.offline.backup.v1/{}",app.id()),&clear)?;
    let mut output=Vec::with_capacity(MAGIC.len()+salt.len()+encrypted.len());
    output.extend_from_slice(MAGIC);output.extend_from_slice(&salt);output.extend_from_slice(&encrypted);
    if output.len()>MAX_BACKUP {return Err(Error::Invalid("backup exceeds size limit"))}
    Ok(output)
}

pub(crate) fn decode(app:App,passphrase:&str,bytes:&[u8])->Result<BackupPayload> {
    if bytes.len()<MAGIC.len()+32+29 || bytes.len()>MAX_BACKUP || &bytes[..MAGIC.len()]!=MAGIC {return Err(Error::Invalid("backup file"))}
    let salt:[u8;32]=bytes[MAGIC.len()..MAGIC.len()+32].try_into().map_err(|_|Error::Invalid("backup file"))?;
    let key=derive(passphrase,&salt)?;
    let clear=Zeroizing::new(key.open(&format!("thinkgrow.offline.backup.v1/{}",app.id()),&bytes[MAGIC.len()+32..]).map_err(|_|Error::Invalid("backup password or file"))?);
    if clear.len()>MAX_CLEAR {return Err(Error::Invalid("backup exceeds size limit"))}
    serde_json::from_slice(&clear).map_err(|_|Error::Invalid("backup file"))
}

pub(crate) fn validate_passphrase(passphrase:&str)->Result<()> {
    if !(12..=128).contains(&passphrase.chars().count()) || passphrase.bytes().any(|byte|byte.is_ascii_control()) {return Err(Error::Invalid("backup passphrase"))} Ok(())
}

fn derive(passphrase:&str,salt:&[u8;32])->Result<DataKey> {
    validate_passphrase(passphrase)?;
    let mut bytes=Zeroizing::new([0;32]);
    Argon2::default().hash_password_into(passphrase.as_bytes(),salt,bytes.as_mut()).map_err(|_|Error::Encryption)?;
    Ok(DataKey::new(*bytes))
}

fn random_salt()->[u8;32] {let mut salt=[0;32];OsRng.fill_bytes(&mut salt);salt}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture()->BackupPayload {
        BackupPayload {
            format:1,
            app_id:App::Student.id().into(),
            handle:"11111111-1111-4111-8111-111111111111".into(),
            label:"학생 계정".into(),
            enrolled:false,
            wall_floor:1_800_000_000_000,
            secrets:Secrets {
                data_key:[3;32],
                signer_seed:[4;32],
                device_id:"22222222-2222-4222-8222-222222222222".into(),
                subject:None,
                grant:None,
            },
            files:vec![BackupFile{path:"store.sqlite3".into(),content:"AA==".into()}],
        }
    }

    #[test]
    fn backup_is_authenticated_and_bound_to_role_and_password() {
        let payload=fixture();
        let encoded=encode(App::Student,"long protected backup phrase",&payload).unwrap();
        let decoded=decode(App::Student,"long protected backup phrase",&encoded).unwrap();
        assert_eq!(decoded.handle,payload.handle);
        assert_eq!(decoded.secrets.device_id,payload.secrets.device_id);
        assert_eq!(decoded.files.len(),1);
        assert!(decode(App::Teacher,"long protected backup phrase",&encoded).is_err());
        assert!(decode(App::Student,"different protected backup phrase",&encoded).is_err());
        let mut tampered=encoded;
        let last=tampered.len()-1;
        tampered[last]^=1;
        assert!(decode(App::Student,"long protected backup phrase",&tampered).is_err());
    }

    #[test]
    fn backup_passphrase_has_a_bounded_length() {
        assert!(validate_passphrase("short phrase").is_ok());
        assert!(validate_passphrase("too short").is_err());
        assert!(validate_passphrase(&"x".repeat(129)).is_err());
    }
}
