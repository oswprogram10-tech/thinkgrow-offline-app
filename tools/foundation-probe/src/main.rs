use std::{env, fs, io::{self, Read}, net::TcpStream, path::Path, time::Duration};
use rusqlite::{Connection, MAIN_DB};
use serde_json::json;
use sha2::{Digest, Sha256};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn database(path: &str) -> Result<Connection> {
    let c = Connection::open(path)?;
    c.busy_timeout(Duration::from_secs(2))?;
    c.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
        PRAGMA foreign_keys=ON; PRAGMA wal_autocheckpoint=0; PRAGMA cache_size=16;")?;
    Ok(c)
}
fn run(a: &[String]) -> Result<()> {
    match a[0].as_str() {
        "db-seed" => {
            let c = database(&a[1])?;
            c.execute_batch("BEGIN IMMEDIATE;
                CREATE TABLE drafts(id TEXT PRIMARY KEY, body TEXT NOT NULL);
                CREATE TABLE outbox(id TEXT PRIMARY KEY, payload TEXT NOT NULL);
                INSERT INTO drafts VALUES('draft-1','preserve-me');
                INSERT INTO outbox VALUES('op-1','pending-before-expiry');
                PRAGMA user_version=1; COMMIT;")?;
            println!("{}", json!({"sqlite": rusqlite::version(), "journal":"wal","synchronous":2}));
        }
        "db-worker" => {
            let c = database(&a[1])?;
            c.execute_batch("BEGIN IMMEDIATE;
                UPDATE drafts SET body='changed-in-transaction' WHERE id='draft-1';
                INSERT INTO outbox VALUES('op-2','second-pending');
                CREATE TABLE crash_spill(data BLOB);
                INSERT INTO crash_spill VALUES(zeroblob(4194304));")?;
            if a[2] == "after-commit" { c.execute_batch("COMMIT;")?; }
            fs::write(&a[3], b"ready")?;
            loop { std::thread::sleep(Duration::from_secs(1)); }
        }
        "db-check" => {
            let c = database(&a[1])?;
            let integrity: String = c.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
            let body: String = c.query_row("SELECT body FROM drafts WHERE id='draft-1'", [], |r| r.get(0))?;
            let count: i64 = c.query_row("SELECT count(*) FROM outbox", [], |r| r.get(0))?;
            let version: i64 = c.query_row("PRAGMA user_version", [], |r| r.get(0))?;
            let migration_column: i64 = c.query_row(
                "SELECT count(*) FROM pragma_table_info('drafts') WHERE name='migration_field'", [], |r| r.get(0))?;
            if integrity != "ok" { return Err("SQLite integrity check failed".into()); }
            println!("{}",json!({"integrity":integrity,"draft":body,"outbox":count,"schema":version,"migration_column":migration_column}));
        }
        "db-backup" => { database(&a[1])?.backup(MAIN_DB, &a[2], rusqlite::backup::NO_PROGRESS)?; }
        "db-failed-migration" => {
            let c = database(&a[1])?;
            c.execute_batch("BEGIN IMMEDIATE; PRAGMA user_version=2;
                ALTER TABLE drafts ADD COLUMN migration_field TEXT;
                INSERT INTO missing_table VALUES(1); COMMIT;")?;
            return Err("Fault injection did not fail".into());
        }
        "credential" => {
            let entry = keyring::Entry::new(&a[2], "synthetic-probe-user")?;
            match a[1].as_str() {
                "set" => { let mut secret=String::new(); io::stdin().read_to_string(&mut secret)?; entry.set_password(&secret)?; }
                "digest" => { let secret=entry.get_password()?; println!("{:x}", Sha256::digest(secret.as_bytes())); }
                "delete" => { entry.delete_credential()?; }
                _ => return Err("Unknown credential operation".into())
            }
        }
        "ws" => {
            let root=native_tls::Certificate::from_pem(&fs::read(&a[1])?)?;
            let connector=native_tls::TlsConnector::builder().disable_built_in_roots(true)
                .add_root_certificate(root).min_protocol_version(Some(native_tls::Protocol::Tlsv12)).build()?;
            let socket=TcpStream::connect(("127.0.0.1", a[2].parse::<u16>()?))?;
            socket.set_read_timeout(Some(Duration::from_secs(5)))?;
            socket.set_write_timeout(Some(Duration::from_secs(5)))?;
            let tls=connector.connect(&a[3], socket)?;
            let url=format!("wss://{}:{}/probe",a[3],a[2]);
            let (mut ws,_)=tungstenite::client(url.as_str(),tls)?;
            let mut request=String::new(); io::stdin().read_to_string(&mut request)?;
            ws.send(tungstenite::Message::Text(request.into()))?;
            match ws.read()? {
                tungstenite::Message::Text(text) => println!("{}",text),
                _ => return Err("No authorized acknowledgement".into())
            }
            let _=ws.close(None);
        }
        "verify" => {
            let key=minisign_verify::PublicKey::from_file(Path::new(&a[1]))?;
            let sig=minisign_verify::Signature::from_file(Path::new(&a[2]))?;
            key.verify(&fs::read(&a[3])?,&sig,false)?;
            println!("verified");
        }
        _ => return Err("Unknown probe command".into())
    }
    Ok(())
}
fn main() {
    let a: Vec<String> = env::args().skip(1).collect();
    if a.is_empty() { eprintln!("probe command required"); std::process::exit(2); }
    if let Err(e)=run(&a) { eprintln!("{e}"); std::process::exit(1); }
}