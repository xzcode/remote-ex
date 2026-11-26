//! Same validated configuration is used by preflight and actual startup.
//! A check never binds a socket, issues credentials or prints secrets.
use std::{collections::HashSet,net::{IpAddr,SocketAddr},time::Duration};
use axum_server::tls_rustls::RustlsConfig;
use remote_ex_signaling::decode_hex;
use remote_ex_rendezvous::turn::IceIssuer;
type Error=Box<dyn std::error::Error>;
struct Settings{
    address:SocketAddr,public:bool,allowed:Option<HashSet<String>>,
    tls:Option<(axum::Router,RustlsConfig)>,stun:bool,turn:bool,relay_count:usize,eligible_relay_count:usize,
}
fn optional(name:&str)->Result<Option<String>,Error>{
    match std::env::var(name){
        Ok(s) if !s.is_empty()=>Ok(Some(s)),
        Ok(_)=>Err(format!("{name} must not be empty").into()),
        Err(std::env::VarError::NotPresent)=>Ok(None),
        Err(_)=>Err(format!("{name} must be valid UTF-8").into()),
    }
}
fn allowlist(path:&str)->Result<HashSet<String>,Error>{Ok(remote_ex_rendezvous::admin::Allowlist::read(std::path::Path::new(path))?.into_set())}
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
    let single_turn=optional("REMOTE_EX_TURN_URL")?.is_some();
    let secret=optional("REMOTE_EX_TURN_SECRET_FILE")?.is_some();
    let pool=optional("REMOTE_EX_RELAY_POOL_FILE")?.is_some();
    let region=optional("REMOTE_EX_RELAY_REGION")?.is_some();
    let turn=single_turn||pool;
    let mut relay_count=0;let mut eligible_relay_count=0;
    let tls=match(cert,key,identity){
        (None,None,None)=>{
            if public||!local||allowed.is_some(){return Err("Enrollment and non-loopback signaling require TLS".into());}
            if stun||turn||secret||region{return Err("ICE configuration requires WSS".into());}None
        }
        (Some(cert),Some(key),Some(identity))=>{
            if !remote_ex_rendezvous::valid_device_id(&identity){return Err("Server identity must be a nonzero lowercase 64-hex ID".into());}
            let pin=decode_hex::<32>(&identity)?;
            let issuer=IceIssuer::from_env().map_err(|_|"Invalid STUN/TURN configuration or unreadable secret file")?;
            relay_count=issuer.relay_count();eligible_relay_count=issuer.eligible_relay_count();
            let app=remote_ex_rendezvous::router_configured(pin,Some(issuer),allowed.clone())?;
            rustls::crypto::ring::default_provider().install_default().map_err(|_|"TLS provider already installed")?;
            let config=RustlsConfig::from_pem_file(cert,key).await.map_err(|_|"TLS certificate/private key cannot be loaded or matched")?;
            Some((app,config))
        }
        _=>return Err("Set all of REMOTE_EX_TLS_CERT, REMOTE_EX_TLS_KEY, and REMOTE_EX_SERVER_ID or none for local-only plaintext development".into()),
    };
    Ok(Settings{address,public,allowed,tls,stun,turn,relay_count,eligible_relay_count})
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
    let args:Vec<String>=std::env::args_os().skip(1).map(|s|s.into_string().map_err(|_|"Arguments must be valid UTF-8")).collect::<std::result::Result<_,_>>()?;
    if let Some(report)=remote_ex_rendezvous::admin::command(&args)?{println!("{report}");return Ok(());}
    let check=match args.as_slice(){
        []=>false,
        [arg] if arg=="--check-config"=>true,
        _=>return Err("Usage: remote-ex-rendezvous [--check-config | --protocol-info | --allowlist-info FILE | --allowlist-diff SOURCE STAGED | --new-allowlist OUTPUT ID KEY | --enroll-plan SOURCE OUTPUT ID KEY | --revoke-plan SOURCE OUTPUT ID]".into()),
    };
    let settings=settings().await?;
    if check{
        println!("{}",serde_json::json!({"schema":1,"valid":true,"mode":if settings.tls.is_some(){"wss"}else{"loopback_ws"},"public_opt_in":settings.public,"enrollment_required":settings.allowed.is_some(),"enrolled_count":settings.allowed.as_ref().map_or(0,HashSet::len),"stun_configured":settings.stun,"turn_configured":settings.turn,"relay_count":settings.relay_count,"eligible_relay_count":settings.eligible_relay_count,"network_started":false}));
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
