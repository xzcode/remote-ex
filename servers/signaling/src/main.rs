//! Loopback development service. TLS mode requires an explicit cert/key and
//! operator-configured public server identity. Never downgrade to plaintext.
use std::{net::{IpAddr, SocketAddr}, time::Duration};
use axum_server::tls_rustls::RustlsConfig;
use remote_ex_signaling::decode_hex;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let address: SocketAddr = std::env::var("REMOTE_EX_SIGNAL_BIND")
        .unwrap_or_else(|_| "127.0.0.1:47822".into()).parse()?;
    if !matches!(address.ip(), IpAddr::V4(ip) if ip.is_loopback()) {
        return Err("Development signaling may bind only to IPv4 loopback".into());
    }
    let cert = std::env::var("REMOTE_EX_TLS_CERT").ok();
    let key = std::env::var("REMOTE_EX_TLS_KEY").ok();
    let identity = std::env::var("REMOTE_EX_SERVER_ID").ok();
    match (cert,key,identity) {
        (None,None,None) => {
            let listener=tokio::net::TcpListener::bind(address).await?;
            println!("Remote EX loopback plaintext development signaling: {}",listener.local_addr()?);
            axum::serve(listener,remote_ex_rendezvous::router())
                .with_graceful_shutdown(async{let _=tokio::signal::ctrl_c().await;}).await?;
        }
        (Some(cert),Some(key),Some(identity))=>{
            let pin=decode_hex::<32>(&identity)?;
            let app=remote_ex_rendezvous::router_with_identity(pin)?;
            rustls::crypto::ring::default_provider().install_default().map_err(|_|"TLS provider already installed")?;
            let config=RustlsConfig::from_pem_file(cert,key).await?;
            let listener=std::net::TcpListener::bind(address)?;
            // Tokio requires a nonblocking listener when adopting std sockets.
            listener.set_nonblocking(true)?;
            let handle=axum_server::Handle::new();
            let shutdown=handle.clone();
            tokio::spawn(async move{
                let _=tokio::signal::ctrl_c().await;
                shutdown.graceful_shutdown(Some(Duration::from_secs(5)));
            });
            println!("Remote EX loopback WSS development signaling: {}",listener.local_addr()?);
            axum_server::from_tcp_rustls(listener,config)?.handle(handle)
                .serve(app.into_make_service()).await?;
        }
        _=>return Err("Set all of REMOTE_EX_TLS_CERT, REMOTE_EX_TLS_KEY, and REMOTE_EX_SERVER_ID or none for local-only plaintext development".into()),
    }
    Ok(())
}
