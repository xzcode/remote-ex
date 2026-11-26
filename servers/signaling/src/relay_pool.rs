//! Bounded administrator-configured TURN pool. Selection is deterministic
//! per route, not controlled by peers. No DNS lookup, probe, health claim,
//! secret logging, automatic failover or mutable routing during a session.
use crate::protocol::ice_url;
use serde::Deserialize;
use sha2::{Digest,Sha256};
use std::{collections::HashSet,io::Read,path::Path};
use zeroize::Zeroizing;
const MAX_FILE:u64=64*1024;
const MAX_RELAYS:usize=8;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileConfig{schema:u32,relays:Vec<RelayConfig>}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RelayConfig{id:String,region:String,priority:u16,url:String,secret_file:String}
struct Relay{id:String,region:String,priority:u16,url:String,secret:Zeroizing<Vec<u8>>}
pub(crate) struct RelayPool{relays:Vec<Relay>,eligible:Vec<usize>}
fn token(s:&str,max:usize)->bool{!s.is_empty()&&s.len()<=max&&s.bytes().all(|b|b.is_ascii_lowercase()||b.is_ascii_digit()||b==b'-')&&s.as_bytes().first().is_some_and(u8::is_ascii_alphanumeric)&&s.as_bytes().last().is_some_and(u8::is_ascii_alphanumeric)}
fn public_dns_shape(host:&str)->bool{
    // A conservative DNS-name syntax gate, not a proof of public resolution.
    // Operators must independently review actual A/AAAA answers and firewall.
    if host.len()>253||host.ends_with('.')||!host.contains('.')||host.parse::<std::net::IpAddr>().is_ok(){return false;}
    let labels:Vec<_>=host.split('.').collect();
    if labels.len()<2||labels.iter().any(|s|!token(s,63)){return false;}
    let last=labels.last().unwrap();
    last.bytes().any(|b|b.is_ascii_lowercase())&&!matches!(*last,"localhost"|"local"|"internal"|"lan"|"home"|"test"|"invalid"|"onion"|"arpa")
}
pub(crate) fn read_secret(path:&Path)->Result<Zeroizing<Vec<u8>>,String>{
    let meta=std::fs::metadata(path).map_err(|_|"Cannot inspect TURN secret file")?;
    if !meta.is_file()||meta.len()>258{return Err("Invalid TURN secret file type or size".into());}
    let mut secret=Zeroizing::new(Vec::new());
    std::fs::File::open(path).map_err(|_|"Cannot open TURN secret file")?.take(258).read_to_end(&mut secret).map_err(|_|"Cannot read TURN secret file")?;
    while matches!(secret.last(),Some(b'\n'|b'\r')){secret.pop();}
    if secret.len()<32||secret.len()>256||!secret.iter().all(u8::is_ascii_graphic){return Err("TURN secret must be 32-256 printable non-space bytes".into());}
    Ok(secret)
}
impl RelayPool{
    pub(crate) fn load(path:&Path,region:Option<&str>)->Result<Self,String>{
        let meta=std::fs::metadata(path).map_err(|_|"Cannot inspect TURN pool file")?;
        if !meta.is_file()||meta.len()==0||meta.len()>MAX_FILE{return Err("Invalid TURN pool file type or size".into());}
        let mut bytes=Vec::new();
        std::fs::File::open(path).map_err(|_|"Cannot open TURN pool file")?.take(MAX_FILE+1).read_to_end(&mut bytes).map_err(|_|"Cannot read TURN pool file")?;
        if bytes.len()>MAX_FILE as usize{return Err("TURN pool exceeds size limit".into());}
        let config:FileConfig=serde_json::from_slice(&bytes).map_err(|_|"Invalid TURN pool JSON")?;
        if config.schema!=1||config.relays.is_empty()||config.relays.len()>MAX_RELAYS||region.is_some_and(|r|!token(r,24)){return Err("Invalid TURN pool schema, count or region".into());}
        let mut ids=HashSet::new();let mut urls=HashSet::new();let mut relays=Vec::new();
        for row in config.relays{
            if !token(&row.id,32)||!token(&row.region,24)||row.priority==0||row.priority>1000||!ids.insert(row.id.clone())||row.secret_file.len()>2048||!Path::new(&row.secret_file).is_absolute(){return Err("Invalid or duplicate TURN pool entry".into());}
            let parsed=ice_url(&row.url,&["turn","turns"])?;
            if !parsed.host_str().is_some_and(public_dns_shape)||!urls.insert(parsed.to_string()){return Err("TURN pool requires distinct reviewed relay DNS names/URLs".into());}
            let secret=read_secret(Path::new(&row.secret_file))?;
            relays.push(Relay{id:row.id,region:row.region,priority:row.priority,url:row.url,secret});
        }
        relays.sort_by(|a,b|a.id.cmp(&b.id));
        let candidates:Vec<_>=relays.iter().enumerate().filter(|(_,r)|region.is_none_or(|v|v==r.region.as_str())).collect();
        let priority=candidates.iter().map(|(_,r)|r.priority).min().ok_or("No configured TURN relay in the selected region")?;
        let eligible=candidates.into_iter().filter(|(_,r)|r.priority==priority).map(|(i,_)|i).collect();
        Ok(Self{relays,eligible})
    }
    pub(crate) fn count(&self)->usize{self.relays.len()}
    pub(crate) fn eligible_count(&self)->usize{self.eligible.len()}
    pub(crate) fn select(&self,session:&str)->(&str,&[u8]){
        let hash=Sha256::digest(session.as_bytes());
        let slot=u64::from_be_bytes(hash[..8].try_into().expect("fixed SHA-256 length"))%(self.eligible.len()as u64);
        let relay=&self.relays[self.eligible[slot as usize]];
        (&relay.url,relay.secret.as_slice())
    }
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn selection_is_region_priority_and_route_stable(){
        let dir=tempfile::tempdir().unwrap();let secret=dir.path().join("secret");
        std::fs::write(&secret,"fixture-not-production-secret-123456789").unwrap();
        let file=dir.path().join("pool.json");
        let row=|id:&str,region:&str,priority:u16,url:&str|serde_json::json!({"id":id,"region":region,"priority":priority,"url":url,"secret_file":secret.to_str().unwrap()});
        let rows=vec![row("a","asia",1,"turns://a.example.net:5349"),row("b","asia",1,"turns://b.example.net:5349"),row("c","asia",2,"turns://c.example.net:5349"),row("d","eu",1,"turns://d.example.net:5349")];
        let write=|rows:&Vec<serde_json::Value>|std::fs::write(&file,serde_json::to_vec(&serde_json::json!({"schema":1,"relays":rows})).unwrap()).unwrap();
        write(&rows);let p=RelayPool::load(&file,Some("asia")).unwrap();
        assert_eq!(p.count(),4);assert_eq!(p.eligible_count(),2);
        for n in 1..=100{let session=format!("{n:032x}");let u=p.select(&session).0;assert!(matches!(u,"turns://a.example.net:5349"|"turns://b.example.net:5349"));assert_eq!(u,p.select(&session).0);}
        let mut reversed=rows.clone();reversed.reverse();write(&reversed);let q=RelayPool::load(&file,Some("asia")).unwrap();assert_eq!(p.select("01").0,q.select("01").0);
        assert!(RelayPool::load(&file,Some("unknown")).is_err());
        for host in ["127.0.0.1","169.254.169.254","relay.local","localhost","relay.internal"]{assert!(!public_dns_shape(host));}
        let bad=vec![row("a","asia",1,"turns://127.0.0.1:5349")];write(&bad);assert!(RelayPool::load(&file,None).is_err());
        let bad=vec![rows[0].clone(),rows[0].clone()];write(&bad);assert!(RelayPool::load(&file,None).is_err());
    }
}
