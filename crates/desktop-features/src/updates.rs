//! Pure release selection and version comparison. Network access belongs
//! to the desktop command. No executable download or installation.
use semver::Version;
use serde::{Deserialize,Serialize};
const PREFIX:&str="Remote-EX-";
pub const RELEASES_PAGE:&str="https://github.com/xzcode/remote-ex/releases";
pub const RELEASES_API:&str="https://api.github.com/repos/xzcode/remote-ex/releases?per_page=20";
#[derive(Deserialize)]
struct RemoteRelease{tag_name:String,draft:bool,prerelease:bool,assets:Vec<RemoteAsset>}
#[derive(Deserialize)]
struct RemoteAsset{name:String,state:String,size:u64}
#[derive(Debug,Clone,Serialize,PartialEq,Eq)]
pub struct Asset{pub name:String,pub size:u64}
#[derive(Debug,Clone,Serialize,PartialEq,Eq)]
pub struct Candidate{pub tag:String,pub prerelease:bool,pub assets:Vec<Asset>}
#[derive(Debug,Clone,Serialize,PartialEq,Eq)]
pub struct Report{pub installed_tag:Option<String>,pub build_commit:Option<String>,pub status:String,pub candidate:Option<Candidate>,pub include_previews:bool}
pub fn version(tag:&str)->Option<Version>{
    if tag.len()>64||!tag.is_ascii(){return None;}
    let v=Version::parse(tag.strip_prefix('v')?).ok()?;
    if !v.build.is_empty()||format!("v{v}")!=tag{return None;}
    if !v.pre.is_empty(){
        let mut ids=v.pre.as_str().split('.');
        if !matches!(ids.next(),Some("alpha"|"beta"|"rc"))||!ids.next().is_some_and(|n|!n.is_empty()&&n.bytes().all(|b|b.is_ascii_digit()))||ids.next().is_some(){return None;}
    }Some(v)
}
fn suffixes(platform:&str,arch:&str)->Option<&'static [&'static str]>{
    match(platform,arch){
        ("windows","x86_64")=>Some(&["Windows-x64.zip","Windows-x64-Setup.exe"]),
        ("macos","aarch64")=>Some(&["macOS-arm64.tar.gz","macOS-arm64.dmg"]),
        ("linux","x86_64")=>Some(&["Linux-x64.tar.gz","Linux-x64.deb"]),
        _=>None,
    }
}
pub fn select(json:&[u8],installed:Option<&str>,commit:Option<&str>,platform:&str,arch:&str,include_previews:bool)->Result<Report,String>{
    if json.len()>1024*1024{return Err("Release metadata exceeds limit".into());}
    let rows:Vec<RemoteRelease>=serde_json::from_slice(json).map_err(|_|"Invalid official release response")?;
    if rows.len()>20{return Err("Too many releases in response".into());}
    let installed=installed.filter(|t|version(t).is_some());
    let commit=commit.filter(|s|s.len()==40&&s.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b)));
    let mut report=Report{installed_tag:installed.map(str::to_owned),build_commit:commit.map(str::to_owned),status:"no_compatible_release".into(),candidate:None,include_previews};
    let Some(suffixes)=suffixes(platform,arch)else{return Ok(report)};
    let mut best:Option<(Version,Candidate)>=None;
    for row in rows{
        if row.draft||row.assets.len()>64{continue;}
        let Some(v)=version(&row.tag_name)else{continue};
        if row.prerelease==v.pre.is_empty()||row.prerelease&&!include_previews{continue;}
        let mut assets=Vec::new();
        for suffix in suffixes{
            let expected=format!("{PREFIX}{}-{suffix}",row.tag_name);
            let mut matching=row.assets.iter().filter(|a|a.name==expected&&a.state=="uploaded"&&a.size>0&&a.size<=1024*1024*1024);
            if let Some(a)=matching.next(){if matching.next().is_none(){assets.push(Asset{name:a.name.clone(),size:a.size});}}
        }
        if assets.is_empty(){continue;}
        if best.as_ref().is_none_or(|(old,_)|v>*old){best=Some((v,Candidate{tag:row.tag_name,prerelease:row.prerelease,assets}));}
    }
    if let Some((v,candidate))=best{
        report.status=match installed.and_then(version){None=>"development",Some(current) if v>current=>"update_available",Some(current) if v==current=>"up_to_date",Some(_)=>"installed_newer"}.into();
        report.candidate=Some(candidate);
    }
    Ok(report)
}
#[cfg(test)]mod tests{
    use super::*;
    fn rows()->Vec<u8>{br#"[{"tag_name":"v0.1.0-alpha.10","draft":false,"prerelease":true,"assets":[{"name":"Remote-EX-v0.1.0-alpha.10-Windows-x64.zip","state":"uploaded","size":1}]},{"tag_name":"v0.1.0","draft":false,"prerelease":false,"assets":[{"name":"Remote-EX-v0.1.0-Windows-x64.zip","state":"uploaded","size":1}]},{"tag_name":"v9.0.0","draft":true,"prerelease":false,"assets":[{"name":"Remote-EX-v9.0.0-Windows-x64.zip","state":"uploaded","size":1}]}]"#.to_vec()}
    #[test]fn semantic_order_and_exact_assets(){
        let x=select(&rows(),Some("v0.1.0-alpha.2"),None,"windows","x86_64",true).unwrap();
        assert_eq!(x.status,"update_available");assert_eq!(x.candidate.unwrap().tag,"v0.1.0");
        assert_eq!(select(&rows(),Some("v0.1.0"),None,"windows","x86_64",false).unwrap().status,"up_to_date");
        assert_eq!(select(&rows(),None,None,"windows","x86_64",true).unwrap().status,"development");
        assert_eq!(select(&rows(),None,None,"macos","aarch64",true).unwrap().status,"no_compatible_release");
        assert!(version("v0.1.0-alpha.10").unwrap()>version("v0.1.0-alpha.2").unwrap());
        for tag in ["latest","v1.0.0+build","v1.0.0-alpha","v1.0.0/../../","v1.0.0-unknown.1"]{assert!(version(tag).is_none());}
    }
}
