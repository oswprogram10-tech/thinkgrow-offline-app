use crate::{Destination, Envelope, Error, Lease, Result, Signed, Store};
pub enum Reply {
    Receipt(Signed),
    Retry{retry_after_ms:Option<u64>},
    AuthenticationRequired,
    Conflict{remote_snapshot:String},
}
/// An adapter must use authenticated native TLS and the registered device proof.
/// No production endpoint is guessed by this library.
pub trait Transport { fn send(&mut self,destination:Destination,envelope:&Envelope)->Result<Reply>; }
#[derive(Debug,PartialEq)]
pub enum Tick { Idle, Received, Deferred, AuthenticationRequired, Conflict }
/// Result of one bounded delivery pass. `remaining` is the still-pending count
/// for the destination after the pass.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct DeliverySummary {
    pub received:u32, pub deferred:u32, pub auth_required:u32, pub conflicts:u32, pub remaining:u64,
}
/// Drain eligible deliveries up to `max` lease attempts. Authentication and
/// conflict outcomes leave the item for a later pass and do not stop the loop.
pub fn deliver(store:&mut Store,transport:&mut dyn Transport,destination:Destination,now:i64,max:usize)->Result<DeliverySummary> {
    let mut summary=DeliverySummary::default();
    for _ in 0..max {
        match tick(store,transport,destination,now)? {
            Tick::Idle=>break,
            Tick::Received=>summary.received=summary.received.saturating_add(1),
            Tick::Deferred=>summary.deferred=summary.deferred.saturating_add(1),
            Tick::AuthenticationRequired=>summary.auth_required=summary.auth_required.saturating_add(1),
            Tick::Conflict=>summary.conflicts=summary.conflicts.saturating_add(1),
        }
    }
    summary.remaining=store.pending(destination)?;
    Ok(summary)
}
pub fn tick(store:&mut Store,transport:&mut dyn Transport,destination:Destination,now:i64)->Result<Tick> {
    let Some(lease)=store.lease(destination,now)? else{return Ok(Tick::Idle)};
    match transport.send(destination,&lease.envelope) {
        Ok(Reply::Receipt(signed))=>{
            if let Err(error)=store.accept_receipt(&signed,destination) {
                retry(store,&lease,now,None)?;
                return Err(error);
            }
            Ok(Tick::Received)
        }
        Ok(Reply::Retry{retry_after_ms})=>{retry(store,&lease,now,retry_after_ms)?;Ok(Tick::Deferred)}
        Ok(Reply::AuthenticationRequired)=>{
            store.delivery_failed(&lease,"auth_wait",0,None)?;Ok(Tick::AuthenticationRequired)
        }
        Ok(Reply::Conflict{remote_snapshot})=>{
            store.delivery_failed(&lease,"conflict",0,Some(&remote_snapshot))?;Ok(Tick::Conflict)
        }
        Err(_)=>{retry(store,&lease,now,None)?;Ok(Tick::Deferred)}
    }
}
fn retry(store:&mut Store,lease:&Lease,now:i64,retry_after:Option<u64>)->Result<()> {
    use rand::Rng;
    let jitter=rand::thread_rng().gen_range(0..1000);
    let delay=retry_after.unwrap_or(1000u64.saturating_mul(1u64<<lease.attempts.min(8))).clamp(1000,86_400_000).saturating_add(jitter);
    let next=now.checked_add(delay as i64).ok_or(Error::Invalid("retry overflow"))?;
    store.delivery_failed(lease,"pending",next,None)
}
