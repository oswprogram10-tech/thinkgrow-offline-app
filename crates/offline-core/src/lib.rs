pub mod contract;
pub mod backup;
pub mod lesson;
pub mod runtime;
pub mod vault;
pub mod registration;
pub mod crypto;
pub mod store;
pub mod transport;
pub mod update;
pub use contract::*;
pub use store::Store;
pub use store::workspace;
pub mod game_host;
pub mod class_materials;
pub mod material_app;
pub use class_materials::{MaterialInfo, MaterialChunk};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid contract: {0}")] Invalid(&'static str),
    #[error("registration service is not configured for this build")] ServiceNotConfigured,
    #[error("secure account storage is unavailable on this platform")] VaultUnavailable,
    #[error("account unlock failed or temporarily locked; data preserved")] AccountLocked,
    #[error("this account has no protected recovery backup")] RecoveryUnavailable,
    #[error("registration request failed; pending device keys preserved")] RegistrationFailed,
    #[error("authorization required")] Authorization,
    #[error("credential expired or clock uncertain; data preserved")] ReadRecoveryOnly,
    #[error("revision conflict; both revisions preserved")] Conflict,
    #[error("signature verification failed")] Signature,
    #[error("encrypted data cannot be opened")] Encryption,
    #[error("account or device partition mismatch")] Partition,
    #[error("unsupported or modified storage schema; original data preserved")] Schema,
    #[error("update deferred until class ends and user confirms")] Deferred,
    #[error("delivery lease lost")] Lease,
    #[error("storage: {0}")] Sql(#[from] rusqlite::Error),
    #[error("file operation failed: {0}")] Io(#[from] std::io::Error),
    #[error("invalid JSON contract")] Json(#[from] serde_json::Error),
}
pub type Result<T> = std::result::Result<T, Error>;
