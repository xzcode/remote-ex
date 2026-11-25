//! Loopback development service. TLS mode requires an explicit cert/key and
//! operator-configured public server identity. Never downgrade to plaintext.
use std::{collections::HashSet,io::Read,net::{IpAddr, SocketAddr}, time::Duration};
use axum_server::tls_rustls::RustlsConfig;
use remote_ex_signaling::decode_hex;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let address: SocketAddr = std::env::var("REMOTE_EX_SIGNAL_BIND")
        .unwrap_or_else(|_| "127.0.0.1:47822".into()).parse()?;
    let local=matches!(address.ip(),IpAddr::V4(ip) if ip.is_loopback());
    let public=std::env::var("REMOTE_EX_PUBLIC_BIND").ok().as_deref()==Some("1");
    if !local&&!public{return Err("Non-loopback bind requires explicit REMOTE_EX_PUBLIC_BIND=1".into());}
    if public&&address.port()==0{return Err("Public deployment requires a fixed nonzero port".into());}
    let allowed=match std::env::var("REMOTE_EX_ALLOWED_DEVICES_FILE").ok(){
        None=>None,
        Some(path)=>{
            let mut text=String::new();std::fs::File::open(path)?.take(512*1024+1).read_to_string(&mut text)?;
            if text.len()>512*1024{return Err("Device allowlist exceeds limit".into());}
            let ids:HashSet<String>=text.lines().map(str::trim).filter(|l|!l.is_empty()&&!l.starts_with('#')).map(str::to_owned).collect();
            if ids.is_empty()||ids.len()>4096||ids.iter().any(|id|!remote_ex_rendezvous::valid_device_id(id)){return Err("Invalid device allowlist".into());}
            Some(ids)
        }
    };
    if public&&allowed.is_none(){return Err("Public signaling requires a nonempty explicit device allowlist".into());}
    let cert = std::env::var("REMOTE_EX_TLS_CERT").ok();
    let key = std::env::var("REMOTE_EX_TLS_KEY").ok();
    let identity = std::env::var("REMOTE_EX_SERVER_ID").ok();
    match (cert,key,identity) {
        (None,None,None) => {
            if public||!local||allowed.is_some(){return Err("Enrollment and non-loopback signaling require TLS".into());}
            if ["REMOTE_EX_STUN_URL","REMOTE_EX_TURN_URL","REMOTE_EX_TURN_SECRET_FILE"].iter().any(|name|std::env::var_os(name).is_some()){
                return Err("ICE configuration requires WSS".into());
            }
            let listener=tokio::net::TcpListener::bind(address).await?;
            println!("Remote EX loopback plaintext development signaling: {}",listener.local_addr()?);
            axum::serve(listener,remote_ex_rendezvous::router())
                .with_graceful_shutdown(async{let _=tokio::signal::ctrl_c().await;}).await?;
        }
        (Some(cert),Some(key),Some(identity))=>{
            let pin=decode_hex::<32>(&identity)?;
            let issuer=remote_ex_rendezvous::turn::IceIssuer::from_env()?;
            let app=remote_ex_rendezvous::router_configured(pin,Some(issuer),allowed)?;
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
            if public{println!("Remote EX enrolled WSS signaling: {}",listener.local_addr()?);}
            else{println!("Remote EX loopback WSS development signaling: {}",listener.local_addr()?);}
            axum_server::from_tcp_rustls(listener,config)?.handle(handle)
                .serve(app.into_make_service()).await?;
        }
        _=>return Err("Set all of REMOTE_EX_TLS_CERT, REMOTE_EX_TLS_KEY, and REMOTE_EX_SERVER_ID or none for local-only plaintext development".into()),
    }
    Ok(())
}
