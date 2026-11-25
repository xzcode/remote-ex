//! Visible, bounded registration diagnostic. No listener or screen/input access.
//! Requests are refused until the actual end-to-end session owner is integrated.
use remote_ex_rendezvous_client::{Connection,Endpoint,Result};
use remote_ex_rendezvous_protocol::{Client,Server};
use remote_ex_security::DeviceIdentity;
use remote_ex_signaling::decode_hex;
use std::{io::{self,IsTerminal,Write,Read},time::{Duration,Instant}};

fn local_identity(persistent:bool)->Result<DeviceIdentity>{
    if !persistent{return Ok(DeviceIdentity::generate());}
    #[cfg(all(feature="persistent",any(target_os="windows",target_os="macos")))]
    {
        if !io::stdin().is_terminal(){return Err("Persistent identity requires an interactive local terminal".into());}
        print!("Use your system credential store for the persistent Remote EX signing key? Type STORE IDENTITY: ");
        io::stdout().flush().map_err(|e|e.to_string())?;
        let mut consent=String::new();
        io::stdin().read_line(&mut consent).map_err(|e|e.to_string())?;
        if consent.trim()!="STORE IDENTITY"{return Err("Persistent identity declined".into());}
        remote_ex_security::identity_store::native::open()
    }
    #[cfg(not(all(feature="persistent",any(target_os="windows",target_os="macos"))))]
    {Err("Persistent identity is not compiled for this platform".into())}
}
fn roots(path:&str)->Result<rustls::RootCertStore>{
    let file=std::fs::File::open(path).map_err(|_|"Cannot open explicitly selected CA file")?;
    let mut data=Vec::new();
    file.take(256*1024+1).read_to_end(&mut data).map_err(|_|"Cannot read CA file")?;
    if data.len()>256*1024{return Err("CA file exceeds limit".into());}
    let mut roots=rustls::RootCertStore::empty();
    for cert in rustls_pemfile::certs(&mut data.as_slice()){
        roots.add(cert.map_err(|_|"Invalid CA certificate")?).map_err(|_|"Invalid CA certificate")?;
    }
    if roots.is_empty(){return Err("No CA certificates in file".into());}
    Ok(roots)
}
async fn run()->Result<()>{
    let mut args=std::env::args().skip(1);
    let mode=args.next().ok_or("Use --dev URL or --wss URL SERVER_ID [--ca PEM] [--persistent]")?;
    let url=args.next().ok_or("Missing server URL")?;
    let endpoint=match mode.as_str(){
        "--dev"=>Endpoint::development(&url)?,
        "--wss"=>Endpoint::secure(&url,decode_hex::<32>(&args.next().ok_or("Missing server ID")?).map_err(|_|"Invalid server ID")?)?,
        _=>return Err("Unknown registration mode".into()),
    };
    let mut persistent=false;let mut ca=None;
    while let Some(arg)=args.next(){
        match arg.as_str(){
            "--persistent" if !persistent&&mode=="--wss"=>persistent=true,
            "--ca" if ca.is_none()&&mode=="--wss"=>ca=Some(args.next().ok_or("Missing CA path")?),
            _=>return Err("Invalid or repeated option (persistent identity requires WSS)".into()),
        }
    }
    let id=local_identity(persistent)?;
    let mut conn=if let Some(path)=ca{Connection::register_with_roots(endpoint,&id,roots(&path)?).await?}
        else{Connection::register(endpoint,&id).await?};
    println!("ONLINE {}",conn.device_id());
    println!("Registration diagnostic only: refuses all connection requests. Ctrl-C stops. Ends after 30 seconds.");
    let stop=Instant::now()+Duration::from_secs(30);
    let mut tick=tokio::time::interval(Duration::from_secs(10));
    loop{
        if Instant::now()>=stop{break;}
        tokio::select!{
            _=tokio::signal::ctrl_c()=>break,
            _=tick.tick()=>{conn.send(Client::Ping).await?;},
            event=conn.receive(Duration::from_secs(20))=>{
                match event?{
                    Server::Pong=>println!("HEARTBEAT OK"),
                    Server::Incoming{session,..}=>{
                        // Do not display or retain peer identifiers in the diagnostic.
                        conn.send(Client::Decision{session,grant:None,host_nonce:None}).await?;
                        println!("REQUEST DECLINED: remote sessions are not enabled here");
                    },
                    Server::Closed{..}=>{},
                    _=>return Err("Unexpected registration response".into()),
                }
            }
        }
    }
    conn.close().await?;
    println!("OFFLINE");Ok(())
}
#[tokio::main]
async fn main(){if let Err(e)=run().await{eprintln!("FAIL: {e}");std::process::exit(1);}}
