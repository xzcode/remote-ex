//! Same validated configuration is used by preflight and actual startup.
//! A check never binds a socket, issues credentials or prints secrets.
use std::{collections::HashSet,io::Read,net::{IpAddr,SocketAddr},time::Duration};
use axum_server::tls_rustls::RustlsConfig;
use remote_ex_signaling::decode_hex;
use remote_ex_rendezvous::turn::IceIssuer;
type Error=Box<dyn std::error::Error>;
struct Settings{
    address:SocketAddr,public:bool,allowed:Option<HashSet<String>>,
    tls:Option<(axum::Router,RustlsConfig)>,stun:bool,turn:bool,
}
fn optional(name:&str)->Result<Option<String>,Error>{
    match std::env::var(name){
        Ok(s) if !s.is_empty()=>Ok(Some(s)),
        Ok(_)=>Err(format!("{name} must not be empty").into()),
        Err(std::env::VarError::NotPresent)=>Ok(None),
        Err(_)=>Err(format!("{name} must be valid UTF-8").into()),
    }
}
fn allowlist(path:&str)->Result<HashSet<String>,Error>{
    let mut text=String::new();
    std::fs::File::open(path).map_err(|_|"Cannot open device allowlist")?.take(512*1024+1).read_to_string(&mut text).map_err(|_|"Cannot read device allowlist")?;
    if text.len()>512*1024{return Err("Device allowlist exceeds limit".into());}
    let ids:HashSet<String>=text.lines().map(str::trim).filter(|l|!l.is_empty()&&!l.starts_with('#')).map(str::to_owned).collect();
    if ids.is_empty()||ids.len()>4096||ids.iter().any(|id|!remote_ex_rendezvous::valid_device_id(id)){return Err("Invalid device allowlist".into());}Ok(ids)
}
async fn settings()->Result<Settings,Error>{
    let address:SocketAddr=optional("REMOTE_EX_SIGNAL_BIND")?.unwrap_or_else(||"127.0.0.1:47822".into()).parse().map_err(|_|"Invalid signaling bind address")?;
    let local=matches!(address.ip(),IpAddr::V4(ip) if ip.is_loopback());
    let public=match optional("REMOTE_EX_PUBLIC_BIND")?.as_deref(){None|Some("0")=>false,Some("1")=>true,_=>return Err("REMOTE_EX_PUBLIC_BIND must be 0 or 1".into())};
    if !local&&!public{return Err("Non-loopback bind requires explicit REMOTE_EX_PUBLIC_BIND=1".into());}
    if public&&address.port()==0{return Err("Public deployment requires a fixed nonzero port".into());}
    let allowed=optional("REMOTE_EX_ALLOWED_DEVICES_FILE")?.map(|p|allowlist(&p)).transpose()?;
    if public&&allowed.is_none(){return Err("Public signaling requires a nonempty explicit device allowlist".into());}
    let cert=optional("REMOTE_EX_TLS_CERT")?;let key=optional("REMOTE_EX_TLS_KEY")?;let identity=optional("REMOTE_EX_SERVER_ID")?;
    let stun=optional("REMOTE_EX_STUN_URL")?.is_some();
    let turn=optional("REMOTE_EX_TURN_URL")?.is_some();
    let secret=optional("REMOTE_EX_TURN_SECRET_FILE")?.is_some();
    let tls=match(cert,key,identity){
        (None,None,None)=>{
            if public||!local||allowed.is_some(){return Err("Enrollment and non-loopback signaling require TLS".into());}
            if stun||turn||secret{return Err("ICE configuration requires WSS".into());}None
        }
        (Some(cert),Some(key),Some(identity))=>{
            if !remote_ex_rendezvous::valid_device_id(&identity){return Err("Server identity must be a nonzero lowercase 64-hex ID".into());}
            let pin=decode_hex::<32>(&identity)?;
            let issuer=IceIssuer::from_env().map_err(|_|"Invalid STUN/TURN configuration or unreadable secret file")?;
            let app=remote_ex_rendezvous::router_configured(pin,Some(issuer),allowed.clone())?;
            rustls::crypto::ring::default_provider().install_default().map_err(|_|"TLS provider already installed")?;
            let config=RustlsConfig::from_pem_file(cert,key).await.map_err(|_|"TLS certificate/private key cannot be loaded or matched")?;
            Some((app,config))
        }
        _=>return Err("Set all of REMOTE_EX_TLS_CERT, REMOTE_EX_TLS_KEY, and REMOTE_EX_SERVER_ID or none for local-only plaintext development".into()),
    };
    Ok(Settings{address,public,allowed,tls,stun,turn})
}
async fn shutdown_signal(){
    #[cfg(unix)]
    {
        if let Ok(mut term)=tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()){
            tokio::select!{_=tokio::signal::ctrl_c()=>{},_=term.recv()=>{}}
            return;
        }
    }
    let _=tokio::signal::ctrl_c().await;
}
#[tokio::main]
async fn main()->Result<(),Error>{
    let mut args=std::env::args_os().skip(1);
    let check=match args.next(){
        None=>false,
        Some(arg) if arg=="--check-config"&&args.next().is_none()=>true,
        _=>return Err("Usage: remote-ex-rendezvous [--check-config]".into()),
    };
    let settings=settings().await?;
    if check{
        println!("{}",serde_json::json!({"schema":1,"valid":true,"mode":if settings.tls.is_some(){"wss"}else{"loopback_ws"},"public_opt_in":settings.public,"enrollment_required":settings.allowed.is_some(),"enrolled_count":settings.allowed.as_ref().map_or(0,HashSet::len),"stun_configured":settings.stun,"turn_configured":settings.turn,"network_started":false}));
        return Ok(());
    }
    match settings.tls{
        None=>{
            let listener=tokio::net::TcpListener::bind(settings.address).await?;
            println!("Remote EX loopback plaintext development signaling: {}",listener.local_addr()?);
            axum::serve(listener,remote_ex_rendezvous::router()).with_graceful_shutdown(shutdown_signal()).await?;
        },
        Some((app,config))=>{
            let listener=std::net::TcpListener::bind(settings.address)?;
            listener.set_nonblocking(true)?;
            let handle=axum_server::Handle::new();let shutdown=handle.clone();
            tokio::spawn(async move{shutdown_signal().await;shutdown.graceful_shutdown(Some(Duration::from_secs(5)));});
            if settings.public{println!("Remote EX enrolled WSS signaling: {}",listener.local_addr()?);}
            else{println!("Remote EX loopback WSS development signaling: {}",listener.local_addr()?);}
            axum_server::from_tcp_rustls(listener,config)?.handle(handle).serve(app.into_make_service()).await?;
        },
    }Ok(())
}
