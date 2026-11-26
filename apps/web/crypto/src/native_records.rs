//! Native v1 record ordering experiment, limited to the synthetic profile.
use super::Result;
use serde::{Deserialize,Serialize};
use remote_ex_security::profiles::MAX_SEALED_CLEAR;
#[derive(Serialize,Deserialize,Debug)]
#[serde(deny_unknown_fields)]
pub(super) struct IceItem{mline:u32,candidate:String}
#[derive(Serialize,Deserialize)]
#[serde(tag="type",rename_all="snake_case",deny_unknown_fields)]
enum Record{Quality{preset:String},IceBatch{items:Vec<IceItem>},Geometry{x:u32,y:u32,width:u32,height:u32},ViewLayout{count:u8}}
fn encode(record:&Record)->Result<Vec<u8>>{
    let b=serde_json::to_vec(record).map_err(|_|"Invalid native fixture record")?;
    if b.len()>MAX_SEALED_CLEAR{return Err("Native fixture record too large".into());}Ok(b)
}
fn decode(data:&[u8])->Result<Record>{
    if data.is_empty()||data.len()>MAX_SEALED_CLEAR{return Err("Native fixture record too large".into());}
    serde_json::from_slice(data).map_err(|_|"Invalid native fixture record".into())
}
fn valid(items:&[IceItem],mode:u8)->bool{
    if mode>2{return false;}
    !items.is_empty()&&items.len()<=128&&items.iter().map(|x|x.candidate.len()).sum::<usize>()<=16*1024
        &&items.iter().any(|x|!x.candidate.is_empty())
        &&items.iter().all(|x|x.mline<=1&&x.candidate.len()<=4096&&!x.candidate.contains(['\r','\n','\0'])
            &&(x.candidate.is_empty()||(x.candidate.starts_with("candidate:")&&x.candidate.split_ascii_whitespace().collect::<Vec<_>>().windows(2).any(|w|w[0]=="typ"&&match mode{0=>w[1]=="host",1=>matches!(w[1],"host"|"srflx"|"prflx"|"relay"),2=>w[1]=="relay",_=>false}))))
}
pub(super) fn quality()->Result<Vec<u8>>{encode(&Record::Quality{preset:"compat".into()})}
pub(super) fn check_quality(data:&[u8])->Result<()>{
    match decode(data)?{Record::Quality{preset} if preset=="compat"=>Ok(()),_=>Err("Unexpected native fixture quality".into())}
}
pub(super) fn ice(json:&str,mode:u8)->Result<Vec<u8>>{
    if json.len()>MAX_SEALED_CLEAR{return Err("ICE input too large".into());}
    let items:Vec<IceItem>=serde_json::from_str(json).map_err(|_|"Invalid ICE fixture items")?;
    if !valid(&items,mode){return Err("ICE candidates exceed selected transport policy".into());}
    encode(&Record::IceBatch{items})
}
pub(super) fn check_ice(data:&[u8],mode:u8)->Result<String>{
    match decode(data)?{Record::IceBatch{items} if valid(&items,mode)=>serde_json::to_string(&items).map_err(|_|"Invalid ICE fixture items".into()),
        _=>Err("Unexpected native fixture ICE".into())}
}
pub(super) fn check_geometry(data:&[u8])->Result<String>{
    match decode(data)?{
        Record::Geometry{x,y,width,height} if width>=2&&height>=2
            &&x.checked_add(width).is_some_and(|n|n<=960)&&y.checked_add(height).is_some_and(|n|n<=540)
            =>serde_json::to_string(&serde_json::json!({"x":x,"y":y,"width":width,"height":height})).map_err(|_|"Invalid geometry".into()),
        _=>Err("View geometry exceeds compat canvas".into())
    }
}
pub(super) fn check_layout(data:&[u8])->Result<u8>{
    match decode(data)?{Record::ViewLayout{count} if (1..=4).contains(&count)=>Ok(count),_=>Err("Invalid authorized display layout".into())}
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn view_metadata_bounds(){
        assert!(check_geometry(br#"{"type":"geometry","x":120,"y":0,"width":720,"height":540}"#).is_ok());
        assert!(check_geometry(br#"{"type":"geometry","x":250,"y":0,"width":720,"height":540}"#).is_err());
        assert!(check_geometry(br#"{"type":"geometry","x":0,"y":0,"width":0,"height":540}"#).is_err());
        assert_eq!(check_layout(br#"{"type":"view_layout","count":4}"#).unwrap(),4);
        assert!(check_layout(br#"{"type":"view_layout","count":5}"#).is_err());
    }
    #[test]fn native_record_shapes_and_rejections(){
        assert_eq!(quality().unwrap(),br#"{"type":"quality","preset":"compat"}"#);
        assert!(check_quality(br#"{"type":"quality","preset":"high"}"#).is_err());
        let item=r#"[{"mline":0,"candidate":"candidate:1 1 UDP 123 127.0.0.1 1234 typ host"}]"#;
        let b=ice(item,0).unwrap();assert_eq!(check_ice(&b,0).unwrap(),item);
        for bad in ["[]",r#"[{"mline":2,"candidate":"candidate:1 1 UDP 123 127.0.0.1 1234 typ host"}]"#,r#"[{"mline":0,"candidate":"candidate:1 1 UDP 123 127.0.0.1 1234 typ relay"}]"#,r#"[{"mline":0,"candidate":"candidate:1 1 UDP 123 127.0.0.1 1234 typ host\n"}]"#]{assert!(ice(bad,0).is_err());}
        assert!(check_ice(&quality().unwrap(),0).is_err());
        let eoc=r#"[{"mline":0,"candidate":"candidate:1 1 UDP 123 127.0.0.1 1234 typ host"},{"mline":0,"candidate":""}]"#;
        assert_eq!(check_ice(&ice(eoc,0).unwrap(),0).unwrap(),eoc);
        assert!(ice(r#"[{"mline":0,"candidate":""}]"#,0).is_err());
        let relay=item.replace("typ host","typ relay");assert!(ice(&relay,0).is_err());assert!(ice(&relay,1).is_ok());assert!(ice(&relay,2).is_ok());assert!(ice(item,2).is_err());
    }
}
