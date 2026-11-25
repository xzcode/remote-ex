//! Intentionally loopback-only until TLS and Internet deployment are reviewed.
use std::net::{IpAddr, SocketAddr};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let address: SocketAddr = std::env::var("REMOTE_EX_SIGNAL_BIND")
        .unwrap_or_else(|_| "127.0.0.1:47822".into()).parse()?;
    if !matches!(address.ip(), IpAddr::V4(ip) if ip.is_loopback()) {
        return Err("Development signaling may bind only to IPv4 loopback".into());
    }
    let listener = tokio::net::TcpListener::bind(address).await?;
    println!("Remote EX development signaling: {}", listener.local_addr()?);
    println!("Loopback only; no TLS or production media sessions.");
    axum::serve(listener, remote_ex_rendezvous::router())
        .with_graceful_shutdown(async { let _ = tokio::signal::ctrl_c().await; })
        .await?;
    Ok(())
}
