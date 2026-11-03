//! Encrypted, account-partitioned class materials vault.
//! Supports PDF, HWP, PPT, Video, Audio, and Applications with chunked resumable download,
//! whole-file SHA-256 verification, and ephemeral decrypted export with automatic lifecycle cleanup.

use super::*;
use base64::{engine::general_purpose::STANDARD, Engine};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use uuid::Uuid;

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MaterialInfo {
    pub id: String,
    pub version: String,
    pub subtype: String,
    #[serde(default)]
    pub app_kind: Option<String>,
    pub title: String,
    pub filename: String,
    pub size: u64,
    pub sha256: String,
    #[serde(default)]
    pub assignment_id: Option<String>,
    #[serde(default)]
    pub teacher_id: Option<String>,
    pub sync_at: i64,
    #[serde(default = "default_true")]
    pub offline_available: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MaterialChunk {
    pub material_id: String,
    pub offset: u64,
    pub length: usize,
    pub total_size: u64,
    pub sha256: String,
    pub filename: String,
    pub chunk_data_b64: String,
}

pub fn sanitize_material_filename(filename: &str) -> String {
    let clean: String = filename
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if clean.is_empty() || clean == "." || clean == ".." {
        "material.bin".to_string()
    } else {
        clean
    }
}

impl Store {
    pub(crate) fn materials_db(&self) -> Result<Connection> {
        let path = self.storage_path().join("materials.sqlite3");
        let exists = path.exists();
        if exists && fs::symlink_metadata(&path)?.file_type().is_symlink() {
            return Err(Error::Partition);
        }
        let mut db = Connection::open(path)?;
        db.busy_timeout(Duration::from_secs(3))?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;")?;

        if !exists {
            let tx = db.transaction()?;
            tx.execute_batch(
                "CREATE TABLE materials (
                    id TEXT PRIMARY KEY,
                    version TEXT NOT NULL,
                    subtype TEXT NOT NULL,
                    app_kind TEXT,
                    title TEXT NOT NULL,
                    filename TEXT NOT NULL,
                    size INTEGER NOT NULL,
                    sha256 TEXT NOT NULL,
                    assignment_id TEXT,
                    teacher_id TEXT,
                    sync_at INTEGER NOT NULL,
                    offline_available INTEGER NOT NULL DEFAULT 1,
                    cipher BLOB NOT NULL
                );
                CREATE TABLE download_chunks (
                    material_id TEXT NOT NULL,
                    offset INTEGER NOT NULL,
                    length INTEGER NOT NULL,
                    chunk_data BLOB NOT NULL,
                    PRIMARY KEY(material_id, offset)
                );
                CREATE TABLE binding (cipher BLOB NOT NULL);
                PRAGMA user_version=1;",
            )?;
            let binding_cipher = self.seal("materials/binding", b"materials-schema-1")?;
            tx.execute("INSERT INTO binding VALUES(?1)", [binding_cipher])?;
            tx.commit()?;
        }

        let binding: Vec<u8> = db.query_row("SELECT cipher FROM binding", [], |r| r.get(0))?;
        if self.decrypt("materials/binding", &binding)? != b"materials-schema-1" {
            return Err(Error::Partition);
        }
        Ok(db)
    }

    pub fn list_materials(&self) -> Result<Vec<MaterialInfo>> {
        let db = self.materials_db()?;
        let mut stmt = db.prepare(
            "SELECT id, version, subtype, app_kind, title, filename, size, sha256, assignment_id, teacher_id, sync_at, offline_available
             FROM materials ORDER BY sync_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            let offline_int: i64 = row.get(11)?;
            Ok(MaterialInfo {
                id: row.get(0)?,
                version: row.get(1)?,
                subtype: row.get(2)?,
                app_kind: row.get(3)?,
                title: row.get(4)?,
                filename: row.get(5)?,
                size: row.get::<_, i64>(6)? as u64,
                sha256: row.get(7)?,
                assignment_id: row.get(8)?,
                teacher_id: row.get(9)?,
                sync_at: row.get(10)?,
                offline_available: offline_int != 0,
            })
        })?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn get_material(&self, id: &str) -> Result<Option<MaterialInfo>> {
        crate::id(id)?;
        let db = self.materials_db()?;
        let mut stmt = db.prepare(
            "SELECT id, version, subtype, app_kind, title, filename, size, sha256, assignment_id, teacher_id, sync_at, offline_available
             FROM materials WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map([id], |row| {
            let offline_int: i64 = row.get(11)?;
            Ok(MaterialInfo {
                id: row.get(0)?,
                version: row.get(1)?,
                subtype: row.get(2)?,
                app_kind: row.get(3)?,
                title: row.get(4)?,
                filename: row.get(5)?,
                size: row.get::<_, i64>(6)? as u64,
                sha256: row.get(7)?,
                assignment_id: row.get(8)?,
                teacher_id: row.get(9)?,
                sync_at: row.get(10)?,
                offline_available: offline_int != 0,
            })
        })?;
        if let Some(res) = rows.next() {
            Ok(Some(res?))
        } else {
            Ok(None)
        }
    }

    pub fn save_material_chunk(&self, chunk: &MaterialChunk, info: &MaterialInfo) -> Result<bool> {
        crate::id(&chunk.material_id)?;
        if !crate::digest(&chunk.sha256) {
            return Err(Error::Invalid("invalid sha256"));
        }
        let chunk_bytes = STANDARD
            .decode(&chunk.chunk_data_b64)
            .map_err(|_| Error::Invalid("chunk encoding"))?;
        if chunk_bytes.len() != chunk.length {
            return Err(Error::Invalid("chunk length mismatch"));
        }

        let mut db = self.materials_db()?;
        let tx = db.transaction()?;
        tx.execute(
            "INSERT OR REPLACE INTO download_chunks (material_id, offset, length, chunk_data) VALUES (?1, ?2, ?3, ?4)",
            params![chunk.material_id, chunk.offset as i64, chunk.length as i64, chunk_bytes],
        )?;

        // Check if all chunks received
        let chunks: Vec<(u64, usize, Vec<u8>)> = {
            let mut stmt = tx.prepare(
                "SELECT offset, length, chunk_data FROM download_chunks WHERE material_id = ?1 ORDER BY offset ASC",
            )?;
            let rows = stmt.query_map([&chunk.material_id], |row| {
                let offset: i64 = row.get(0)?;
                let length: i64 = row.get(1)?;
                let data: Vec<u8> = row.get(2)?;
                Ok((offset as u64, length as usize, data))
            })?;
            let mut list = Vec::new();
            for r in rows {
                list.push(r?);
            }
            list
        };

        let mut assembled = Vec::new();
        let mut expected_offset: u64 = 0;
        for (off, len, data) in chunks {
            if off != expected_offset {
                // Gap in chunks: still in progress
                tx.commit()?;
                return Ok(false);
            }
            assembled.extend_from_slice(&data);
            expected_offset += len as u64;
        }

        if expected_offset < chunk.total_size {
            tx.commit()?;
            return Ok(false);
        }

        if expected_offset != chunk.total_size {
            return Err(Error::Invalid("downloaded size exceeds total"));
        }

        // Full file assembled: verify hash
        let actual_hash = crypto::hash(&assembled);
        if actual_hash != chunk.sha256 {
            // Tampered or corrupted download; clean up chunks and reject
            tx.execute("DELETE FROM download_chunks WHERE material_id = ?1", [&chunk.material_id])?;
            tx.commit()?;
            return Err(Error::Signature);
        }

        // Seal into encrypted vault
        let cipher = self.seal(&format!("materials/{}", chunk.material_id), &assembled)?;
        tx.execute(
            "INSERT OR REPLACE INTO materials (id, version, subtype, app_kind, title, filename, size, sha256, assignment_id, teacher_id, sync_at, offline_available, cipher)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                info.id,
                info.version,
                info.subtype,
                info.app_kind,
                info.title,
                info.filename,
                info.size as i64,
                info.sha256,
                info.assignment_id,
                info.teacher_id,
                info.sync_at,
                if info.offline_available { 1 } else { 0 },
                cipher
            ],
        )?;

        // Remove intermediate chunks
        tx.execute("DELETE FROM download_chunks WHERE material_id = ?1", [&chunk.material_id])?;
        tx.commit()?;
        Ok(true)
    }

    pub fn save_material_blob(&self, info: &MaterialInfo, data: &[u8]) -> Result<()> {
        crate::id(&info.id)?;
        if !crate::digest(&info.sha256) {
            return Err(Error::Invalid("invalid sha256"));
        }
        if crypto::hash(data) != info.sha256 {
            return Err(Error::Signature);
        }
        let cipher = self.seal(&format!("materials/{}", info.id), data)?;
        let db = self.materials_db()?;
        db.execute(
            "INSERT OR REPLACE INTO materials (id, version, subtype, app_kind, title, filename, size, sha256, assignment_id, teacher_id, sync_at, offline_available, cipher)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                info.id,
                info.version,
                info.subtype,
                info.app_kind,
                info.title,
                info.filename,
                info.size as i64,
                info.sha256,
                info.assignment_id,
                info.teacher_id,
                info.sync_at,
                if info.offline_available { 1 } else { 0 },
                cipher
            ],
        )?;
        Ok(())
    }

    pub fn decrypt_material(&self, id: &str) -> Result<(MaterialInfo, Vec<u8>)> {
        crate::id(id)?;
        let db = self.materials_db()?;
        let (info, cipher): (MaterialInfo, Vec<u8>) = db.query_row(
            "SELECT id, version, subtype, app_kind, title, filename, size, sha256, assignment_id, teacher_id, sync_at, offline_available, cipher
             FROM materials WHERE id = ?1",
            [id],
            |row| {
                let offline_int: i64 = row.get(11)?;
                let info = MaterialInfo {
                    id: row.get(0)?,
                    version: row.get(1)?,
                    subtype: row.get(2)?,
                    app_kind: row.get(3)?,
                    title: row.get(4)?,
                    filename: row.get(5)?,
                    size: row.get::<_, i64>(6)? as u64,
                    sha256: row.get(7)?,
                    assignment_id: row.get(8)?,
                    teacher_id: row.get(9)?,
                    sync_at: row.get(10)?,
                    offline_available: offline_int != 0,
                };
                let cipher: Vec<u8> = row.get(12)?;
                Ok((info, cipher))
            },
        )?;

        let plain = self.decrypt(&format!("materials/{}", info.id), &cipher)?;
        if crypto::hash(&plain) != info.sha256 {
            return Err(Error::Signature);
        }
        Ok((info, plain))
    }

    pub fn delete_material(&self, id: &str) -> Result<()> {
        crate::id(id)?;
        let mut db = self.materials_db()?;
        let tx = db.transaction()?;
        tx.execute("DELETE FROM materials WHERE id = ?1", [id])?;
        tx.execute("DELETE FROM download_chunks WHERE material_id = ?1", [id])?;
        tx.commit()?;
        Ok(())
    }

    pub fn export_ephemeral_material(&self, id: &str) -> Result<(MaterialInfo, PathBuf)> {
        let (info, plain) = self.decrypt_material(id)?;
        let safe_name = sanitize_material_filename(&info.filename);
        let temp_dir = std::env::temp_dir().join(format!("tg-mat-{}", Uuid::new_v4().simple()));
        fs::create_dir_all(&temp_dir)?;
        let target_path = temp_dir.join(safe_name);
        fs::write(&target_path, &plain)?;
        Ok((info, target_path))
    }
}
