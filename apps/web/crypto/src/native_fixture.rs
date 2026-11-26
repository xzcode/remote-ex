//! Disposable, fixed stdin/stdout exchange for native-vs-WASM testing.
//! Never included in the browser package. No network, real identity, input,
//! arbitrary plaintext, filesystem requests or long-lived service.
use super::*;
use rand_core::{OsRng,RngCore};
use serde::{Deserialize,Serialize,de::DeserializeOwned};
use std::io::{BufRead,Read,Write};
const MAX_LINE:u64=8192;
#[derive(Serialize)]struct Hello{host:Vec<u8>,nonce:Vec<u8>}
#[derive(Deserialize)]#[serde(deny_unknown_fields)]
struct Start{controller:Vec<u8>,session:Vec<u8>,nonce:Vec<u8>,handshake:Vec<u8>}
#[derive(Serialize)]struct Response{handshake:Vec<u8>,proof:Vec<u8>}
#[derive(Deserialize)]#[serde(deny_unknown_fields)]
struct Finish{proof:Vec<u8>,record:Vec<u8>}
#[derive(Serialize)]struct Done{record:Vec<u8>,fixture_verified:bool}
fn receive<T:DeserializeOwned>(reader:&mut impl BufRead)->Result<T>{
    let mut line=String::new();
    let n=reader.take(MAX_LINE+1).read_line(&mut line).map_err(|_|"Fixture input unavailable")?;
    if n==0||n as u64>MAX_LINE||!line.ends_with('\n'){return Err("Invalid fixture input length".into());}
    serde_json::from_str(&line).map_err(|_|"Invalid fixed fixture request".into())
}
fn send<T:Serialize>(writer:&mut impl Write,value:&T)->Result<()>{
    serde_json::to_writer(&mut *writer,value).map_err(|_|"Cannot encode fixture output")?;
    writer.write_all(b"\n").and_then(|_|writer.flush()).map_err(|_|"Cannot send fixture output".into())
}
pub fn run_native_fixture()->Result<()>{
    let id=remote_ex_security::DeviceIdentity::generate();
    let mut nonce=[0;32];OsRng.fill_bytes(&mut nonce);
    let stdin=std::io::stdin();let mut input=stdin.lock();
    let stdout=std::io::stdout();let mut output=stdout.lock();
    send(&mut output,&Hello{host:id.public_key().to_vec(),nonce:nonce.to_vec()})?;
    let req:Start=receive(&mut input)?;
    let mut h=Peer::new(Role::Host,&req.session,&req.controller,&id.public_key(),&req.nonce,&nonce)?;
    h.read(&req.handshake)?;let handshake=h.write()?;
    let sig=h.binding.sign(&id,Role::Host,&h.hash.ok_or("Native fixture handshake incomplete")?).map_err(|_|"Native fixture proof failed")?;
    let proof=h.send_proof(&sig)?;
    send(&mut output,&Response{handshake,proof})?;
    let finish:Finish=receive(&mut input)?;
    h.receive_proof(&finish.proof)?;h.receive_fixture(&finish.record)?;
    let record=h.send_fixture()?;h.close();
    send(&mut output,&Done{record,fixture_verified:true})
}
