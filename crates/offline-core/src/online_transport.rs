//! Native HTTPS transport for direct online submission.
//!
//! The server contract is shared with the installed-app receiver: a credential
//! and a frozen envelope go up, a signed online receipt comes back. No endpoint
//! is guessed here; the caller supplies the fixed service origin.
use crate::{Destination, Envelope, Error, Result, Signed};
use crate::transport::{Reply, Transport};
use serde::Deserialize;

const MAX_RECEIPT_BODY: usize = 64 * 1024;
const MAX_CONFLICT_BODY: usize = 16 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReceiptReply {
    receipt: Signed,
}

pub struct HttpTransport {
    origin: String,
    credential: Signed,
}

impl HttpTransport {
    pub fn new(origin: String, credential: Signed) -> Self {
        Self { origin, credential }
    }
}

impl Transport for HttpTransport {
    fn send(&mut self, destination: Destination, envelope: &Envelope) -> Result<Reply> {
        if destination != Destination::Online {
            return Err(Error::Invalid("destination"));
        }
        send(&self.origin, &self.credential, envelope)
    }
}

/// Pure mapping from the shared status table. Kept separate from the network
/// call so it can be tested without a server.
pub fn map_response(status: u16, retry_after: Option<&str>, body: &[u8]) -> Result<Reply> {
    match status {
        200 => {
            if body.len() > MAX_RECEIPT_BODY {
                return Err(Error::RegistrationFailed);
            }
            let reply: ReceiptReply =
                serde_json::from_slice(body).map_err(|_| Error::RegistrationFailed)?;
            Ok(Reply::Receipt(reply.receipt))
        }
        401 => Ok(Reply::AuthenticationRequired),
        400 | 403 | 409 => {
            let limit = body.len().min(MAX_CONFLICT_BODY);
            let parsed: Option<serde_json::Value> = serde_json::from_slice(&body[..limit]).ok();
            if let Some(snapshot) = parsed
                .as_ref()
                .and_then(|value| value.get("remote_snapshot"))
                .and_then(|value| value.as_str())
            {
                return Ok(Reply::Conflict {
                    remote_snapshot: snapshot.to_owned(),
                });
            }
            let code = parsed
                .as_ref()
                .and_then(|value| value.get("code"))
                .and_then(|value| value.as_str())
                .unwrap_or("");
            let encoded = serde_json::to_string(code).map_err(|_| Error::RegistrationFailed)?;
            Ok(Reply::Conflict {
                remote_snapshot: format!("{{\"status\":{status},\"code\":{encoded}}}"),
            })
        }
        429 | 503 => Ok(Reply::Retry {
            retry_after_ms: parse_retry_after(retry_after),
        }),
        _ => Err(Error::RegistrationFailed),
    }
}

fn parse_retry_after(value: Option<&str>) -> Option<u64> {
    value
        .and_then(|value| value.trim().parse::<u64>().ok())
        .map(|seconds| seconds.saturating_mul(1000))
}

#[cfg(windows)]
fn send(origin: &str, credential: &Signed, envelope: &Envelope) -> Result<Reply> {
    use std::{io::Read, time::Duration};
    let client = reqwest::blocking::Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|_| Error::RegistrationFailed)?;
    let request = serde_json::json!({"credential": credential, "envelope": envelope});
    let response = client
        .post(format!("{origin}/api/offline/v1/submissions"))
        .json(&request)
        .send()
        .map_err(|_| Error::RegistrationFailed)?;
    let status = response.status().as_u16();
    let retry_after = response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let limit = match status {
        200 => MAX_RECEIPT_BODY,
        400 | 403 | 409 => MAX_CONFLICT_BODY,
        _ => 0,
    };
    if limit == 0 {
        return map_response(status, retry_after.as_deref(), &[]);
    }
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(Error::RegistrationFailed);
    }
    let mut raw = Vec::new();
    response
        .take(limit as u64 + 1)
        .read_to_end(&mut raw)
        .map_err(|_| Error::RegistrationFailed)?;
    if raw.len() > limit {
        return Err(Error::RegistrationFailed);
    }
    map_response(status, retry_after.as_deref(), &raw)
}

#[cfg(not(windows))]
fn send(_origin: &str, _credential: &Signed, _envelope: &Envelope) -> Result<Reply> {
    Err(Error::VaultUnavailable)
}
