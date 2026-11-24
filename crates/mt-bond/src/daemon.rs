//! The merge daemon (`mt-merge-server`, D17): the server-side half of
//! stream bonding.
//!
//! Listens on a loopback port on the tunnel server. Carriers arrive as
//! plain TCP connections forwarded by the local Xray: each per-adapter
//! engine's VLESS connection targets `127.0.0.1:<port>`, Xray's freedom
//! outbound dials it, and the daemon sees the inner stream. HELLO frames
//! group carriers into bonds; OPEN frames announce sessions; the daemon
//! dials real targets and pumps bytes both ways, splitting reverse
//! traffic back across all carriers of the bond.
//!
//! Security model: loopback-only by default. Authentication happened at
//! the REALITY+UUID edge; anything that can speak to this port is already
//! inside the server. (Documented in D17 §6.)

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, watch};

use mt_net::{Host, ProxyStream, TargetAddr};

use crate::endpoint::{Endpoint, Inbound};
use crate::frame::{self, CloseReason, Kind};

/// A bond with no live carriers for this long is garbage-collected.
pub const BOND_IDLE_GC: Duration = Duration::from_secs(60);
/// How long the daemon waits for a carrier's HELLO.
const HELLO_TIMEOUT: Duration = Duration::from_secs(10);
/// GC sweep cadence.
const GC_INTERVAL: Duration = Duration::from_secs(15);

/// Poison-tolerant lock (repo rule).
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

struct BondEntry {
    endpoint: Arc<Endpoint>,
    /// Last time a carrier attached (GC reference point).
    last_live: Instant,
}

type BondTable = Arc<Mutex<HashMap<u32, BondEntry>>>;

/// Run the merge daemon until `shutdown` fires. Fails if the listener
/// cannot bind.
pub async fn serve(listen: SocketAddr, mut shutdown: watch::Receiver<bool>) -> anyhow::Result<()> {
    let listener = TcpListener::bind(listen).await?;
    eprintln!("[mt-merge-server] listening on {listen}");
    let bonds: BondTable = Arc::new(Mutex::new(HashMap::new()));
    let gc = tokio::spawn(gc_loop(Arc::clone(&bonds)));
    let result = accept_loop(&listener, &bonds, &mut shutdown).await;
    gc.abort();
    // Tear every bond down.
    let entries: Vec<Arc<Endpoint>> = lock(&bonds)
        .values()
        .map(|e| Arc::clone(&e.endpoint))
        .collect();
    for ep in entries {
        let _ = ep.shutdown_handle().send(true);
    }
    result
}

async fn accept_loop(
    listener: &TcpListener,
    bonds: &BondTable,
    shutdown: &mut watch::Receiver<bool>,
) -> anyhow::Result<()> {
    loop {
        let (conn, _peer) = tokio::select! {
            biased;
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    return Ok(());
                }
                continue;
            }
            accepted = listener.accept() => accepted?,
        };
        let bonds = Arc::clone(bonds);
        tokio::spawn(async move {
            // S21 P1 perf: the carrier leg is bulk+latency mixed; Nagle on
            // the public-internet carrier turns each 16 KiB frame's tail
            // segment into a delayed-ACK stall (~40 ms per frame on Linux
            // peers) — TCP_NODELAY, matching the client side.
            let _ = conn.set_nodelay(true);
            if let Err(e) = handle_conn(bonds, conn).await {
                tracing::debug!("bond carrier connection ended: {e}");
            }
        });
    }
}

/// One accepted connection: expect HELLO, join/create the bond, attach.
async fn handle_conn(bonds: BondTable, mut conn: TcpStream) -> anyhow::Result<()> {
    let hello = tokio::time::timeout(HELLO_TIMEOUT, frame::read_frame(&mut conn))
        .await
        .map_err(|_| anyhow::anyhow!("carrier HELLO timed out"))??;
    if hello.kind != Kind::Hello {
        anyhow::bail!("first carrier frame was {:?}, expected HELLO", hello.kind);
    }
    let name = String::from_utf8_lossy(&hello.payload).to_string();
    let bond_id = hello.bond_id;
    // Join or create the bond.
    let (endpoint, created) = {
        let mut table = lock(&bonds);
        match table.get_mut(&bond_id) {
            Some(entry) => {
                entry.last_live = Instant::now();
                (Arc::clone(&entry.endpoint), false)
            }
            None => {
                let endpoint = Endpoint::server(bond_id);
                let (tx, rx) = mpsc::unbounded_channel();
                endpoint.set_session_notifier(tx);
                tokio::spawn(Arc::clone(&endpoint).sweeper_task());
                tokio::spawn(bond_pumps(Arc::clone(&endpoint), rx));
                table.insert(
                    bond_id,
                    BondEntry {
                        endpoint: Arc::clone(&endpoint),
                        last_live: Instant::now(),
                    },
                );
                (endpoint, true)
            }
        }
    };
    let stream: ProxyStream = Box::new(conn);
    endpoint.attach_carrier(stream, name, None).await?;
    if created {
        tracing::info!(bond = bond_id, "new bond established");
    }
    Ok(())
}

/// Per-bond session pump supervisor: consumes new-session notifications
/// and spawns one pump per session.
async fn bond_pumps(endpoint: Arc<Endpoint>, mut new_sessions: mpsc::UnboundedReceiver<u32>) {
    while let Some(id) = new_sessions.recv().await {
        let Some(rx) = endpoint.take_inbound(id) else {
            continue;
        };
        let ep = Arc::clone(&endpoint);
        tokio::spawn(async move {
            if let Err(e) = session_pump(ep, id, rx).await {
                tracing::debug!(session = id, error = %e, "bond session pump ended");
            }
        });
    }
}

/// One session: wait for OPEN → dial → pump bytes both directions.
async fn session_pump(
    endpoint: Arc<Endpoint>,
    id: u32,
    mut rx: mpsc::UnboundedReceiver<Inbound>,
) -> anyhow::Result<()> {
    // 1. The OPEN (always the first in-order delivery).
    let target = match rx.recv().await {
        Some(Inbound::Opened(t)) => t,
        Some(Inbound::Eof(reason)) => {
            // Client gave up before the OPEN arrived.
            if let Some(s) = endpoint.get_session(id) {
                endpoint.close_session(&s, reason, false).await;
            }
            return Ok(());
        }
        _ => return Ok(()),
    };
    let session = endpoint
        .get_session(id)
        .ok_or_else(|| anyhow::anyhow!("session {id} vanished before dial"))?;
    // 2. Dial the real target.
    let dialed = tokio::time::timeout(crate::endpoint::DIAL_TIMEOUT, dial_target(&target)).await;
    let stream = match dialed {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            tracing::info!(session = id, target = %target, error = %e, "bond dial failed");
            endpoint
                .close_session(&session, CloseReason::Refused, true)
                .await;
            return Ok(());
        }
        Err(_) => {
            tracing::info!(session = id, target = %target, "bond dial timed out");
            endpoint
                .close_session(&session, CloseReason::Refused, true)
                .await;
            return Ok(());
        }
    };
    let (mut target_read, mut target_write) = stream.into_split();
    // 3. Upstream pump: inbound (client→server) → target writes. A clean
    //    client write-close half-closes the target write side so the
    //    target can finish its response; other reasons abort.
    let mut up = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            match msg {
                Inbound::Data(bytes) => {
                    if target_write.write_all(&bytes).await.is_err() {
                        return CloseReason::Error;
                    }
                }
                Inbound::Opened(_) => continue,
                Inbound::Eof(reason) => {
                    let _ = target_write.shutdown().await;
                    return reason;
                }
            }
        }
        // rx dropped (session removed) — half-close whatever remains.
        let _ = target_write.shutdown().await;
        CloseReason::Eof
    });
    // 4. Downstream pump: target reads → DATA frames across the carriers.
    let down_endpoint = Arc::clone(&endpoint);
    let down_session = Arc::clone(&session);
    let mut down = tokio::spawn(async move {
        let mut buf = vec![0u8; frame::MAX_PAYLOAD];
        loop {
            match target_read.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if down_endpoint
                        .send_data(&down_session, &buf[..n])
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            }
        }
    });
    // 5. Wait for the direction that ends first:
    //    - client write-close (up returns) → keep draining the target
    //      until EOF, then final-close toward the client;
    //    - target EOF first (down returns) → final-close toward the
    //      client right away (removing the session delivers Eof to the
    //      up pump, which half-closes the target and exits).
    let up_reason = tokio::select! {
        reason = &mut up => reason.unwrap_or(CloseReason::Error),
        _ = &mut down => {
            endpoint.close_session(&session, CloseReason::Eof, true).await;
            let _ = up.await;
            return Ok(());
        }
    };
    if up_reason != CloseReason::Eof {
        down.abort();
        endpoint.close_session(&session, up_reason, false).await;
        return Ok(());
    }
    let _ = down.await;
    // Both directions done — final close (flushes reverse data first).
    endpoint
        .close_session(&session, CloseReason::Eof, true)
        .await;
    Ok(())
}

/// Resolve + dial a target (no dependency on a resolver crate: tokio's
/// `ToSocketAddrs` handles domains via getaddrinfo).
async fn dial_target(target: &TargetAddr) -> anyhow::Result<TcpStream> {
    match &target.host {
        Host::Domain(d) => {
            let stream = TcpStream::connect((d.as_str(), target.port)).await?;
            // S21 P1 perf: Nagle here stalls the session pump's 16 KiB
            // writes behind the target's delayed ACK.
            let _ = stream.set_nodelay(true);
            Ok(stream)
        }
        Host::Ip(ip) => {
            let stream = TcpStream::connect(SocketAddr::new(*ip, target.port)).await?;
            let _ = stream.set_nodelay(true);
            Ok(stream)
        }
    }
}

/// Periodic GC: drop bonds whose carriers are all dead for BOND_IDLE_GC.
async fn gc_loop(bonds: BondTable) {
    let mut tick = tokio::time::interval(GC_INTERVAL);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tick.tick().await;
        let mut table = lock(&bonds);
        let dead: Vec<u32> = table
            .iter()
            .filter(|(_, e)| e.endpoint.all_carriers_dead() && e.last_live.elapsed() > BOND_IDLE_GC)
            .map(|(id, _)| *id)
            .collect();
        for id in dead {
            if let Some(entry) = table.remove(&id) {
                tracing::info!(bond = id, "bond idle-collected");
                let _ = entry.endpoint.shutdown_handle().send(true);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn serve_binds_and_shuts_down() {
        let listener_probe = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener_probe.local_addr().unwrap();
        drop(listener_probe);
        let (tx, rx) = watch::channel(false);
        let handle = tokio::spawn(serve(addr, rx));
        // Give the listener a moment to bind.
        tokio::time::sleep(Duration::from_millis(200)).await;
        tx.send(true).unwrap();
        handle
            .await
            .expect("serve task")
            .expect("serve returns Ok on shutdown");
    }
}
