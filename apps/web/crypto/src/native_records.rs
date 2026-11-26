//! Native v1 record ordering experiment, limited to the synthetic profile.
use super::Result;
use serde::{Deserialize,Serialize};
use remote_ex_security::profiles::MAX_SEALED_CLEAR;
#[derive(Serialize,Deserialize,Debug)]
#[serde(deny_unknown_fields)]
pub(super) struct IceItem{mline:u32,candidate:String}
#[derive(Serialize,Deserialize)]
#[serde(tag="type",rename_all="snake_case",deny_unknown_fields)]
enum Record{Quality{preset:String},IceBatch{items:Vec<IceItem>}}
fn encode(record:&Record)->Result<Vec<u8>>{
    let b=serde_json::to_vec(record).map_err(|_|"Invalid native fixture record")?;
    if b.len()>MAX_SEALED_CLEAR{return Err("Native fixture record too large".into());}Ok(b)
}
fn decode(data:&[u8])->Result<Record>{
    if data.is_empty()||data.len()>MAX_SEALED_CLEAR{return Err("Native fixture record too large".into());}
    serde_json::from_slice(data).map_err(|_|"Invalid native fixture record".into())
}
fn valid(items:&[IceItem])->bool{
    !items.is_empty()&&items.len()<=128&&items.iter().map(|x|x.candidate.len()).sum::<usize>()<=16*1024
        &&items.iter().all(|x|x.mline<=1&&x.candidate.len()<=4096&&!x.candidate.contains(['\r','\n','\0'])
            &&x.candidate.starts_with("candidate:")&&x.candidate.split_ascii_whitespace().collect::<Vec<_>>().windows(2).any(|w|w==["typ","host"]))
}
pub(super) fn quality()->Result<Vec<u8>>{encode(&Record::Quality{preset:"compat".into()})}
pub(super) fn check_quality(data:&[u8])->Result<()>{
    match decode(data)?{Record::Quality{preset} if preset=="compat"=>Ok(()),_=>Err("Unexpected native fixture quality".into())}
}
pub(super) fn ice(json:&str)->Result<Vec<u8>>{
    if json.len()>MAX_SEALED_CLEAR{return Err("ICE input too large".into());}
    let items:Vec<IceItem>=serde_json::from_str(json).map_err(|_|"Invalid ICE fixture items")?;
    if !valid(&items){return Err("Only bounded host candidates permitted in native fixture".into());}
    encode(&Record::IceBatch{items})
}
pub(super) fn check_ice(data:&[u8])->Result<String>{
    match decode(data)?{Record::IceBatch{items} if valid(&items)=>serde_json::to_string(&items).map_err(|_|"Invalid ICE fixture items".into()),
        _=>Err("Unexpected native fixture ICE".into())}
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn native_record_shapes_and_rejections(){
        assert_eq!(quality().unwrap(),br#"{"type":"quality","preset":"compat"}"#);
        assert!(check_quality(br#"{"type":"quality","preset":"high"}"#).is_err());
        let item=r#"[{"mline":0,"candidate":"candidate:1 1 UDP 123 127.0.0.1 1234 typ host"}]"#;
        let b=ice(item).unwrap();assert_eq!(check_ice(&b).unwrap(),item);
        for bad in ["[]",r#"[{"mline":2,"candidate":"candidate:1 1 UDP 123 127.0.0.1 1234 typ host"}]"#,r#"[{"mline":0,"candidate":"candidate:1 1 UDP 123 127.0.0.1 1234 typ relay"}]"#,r#"[{"mline":0,"candidate":"candidate:1 1 UDP 123 127.0.0.1 1234 typ host\n"}]"#]{assert!(ice(bad).is_err());}
        assert!(check_ice(&quality().unwrap()).is_err());
    }
}
