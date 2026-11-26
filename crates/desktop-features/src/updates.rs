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
struct RemoteAsset{name:String,state:String,size:u64,#[serde(default)]digest:Option<String>}
#[derive(Debug,Clone,Serialize,PartialEq,Eq)]
pub struct Asset{pub name:String,pub size:u64,pub sha256:Option<String>}
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
fn digest(value:Option<&str>)->Option<String>{
    let s=value?.strip_prefix("sha256:")?;
    (s.len()==64&&s.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b))).then(||s.to_owned())
}
/// Read only one explicitly selected archive, bounded by official metadata.
/// Does not unpack, execute or install it. A matching digest is not a signature.
pub fn check_archive(path:&std::path::Path,asset:&Asset)->Result<bool,String>{
    use std::{io::Read,time::{Instant,Duration}};
    use sha2::{Digest,Sha256};
    let expected=asset.sha256.as_deref().ok_or("No official checksum available")?;
    if digest(Some(&format!("sha256:{expected}"))).is_none()||asset.size==0||asset.size>1024*1024*1024{return Err("Invalid release asset metadata".into());}
    if !path.is_absolute()||path.file_name().and_then(|s|s.to_str())!=Some(asset.name.as_str()){return Err("Choose the exact listed release archive filename".into());}
    let meta=std::fs::symlink_metadata(path).map_err(|_|"Cannot inspect selected archive")?;
    if !meta.file_type().is_file()||meta.len()!=asset.size{return Err("Archive size or file type does not match metadata".into());}
    let mut file=std::fs::File::open(path).map_err(|_|"Cannot open selected archive")?;
    if !file.metadata().map_err(|_|"Cannot read archive metadata")?.is_file(){return Err("Selected archive is not a regular file".into());}
    let mut left=asset.size;let mut hash=Sha256::new();let mut chunk=vec![0u8;64*1024];let start=Instant::now();
    while left>0{
        if start.elapsed()>Duration::from_secs(60){return Err("Archive verification timed out".into());}
        let limit=chunk.len().min(left as usize);
        let n=file.read(&mut chunk[..limit]).map_err(|_|"Cannot read selected archive")?;
        if n==0{return Err("Archive ended before its published size".into());}
        hash.update(&chunk[..n]);left-=n as u64;
    }
    if file.read(&mut chunk[..1]).map_err(|_|"Cannot finish archive check")?!=0{return Err("Archive changed while being read".into());}
    Ok(remote_ex_signaling::hex(&hash.finalize())==expected)
}
/// Build only exact official release-page URLs, never trust a URL from remote metadata.
pub fn release_page(tag:&str)->Result<String,String>{
    version(tag).ok_or("Invalid published release tag")?;
    Ok(format!("{RELEASES_PAGE}/tag/{tag}"))
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
            if let Some(a)=matching.next(){if matching.next().is_none(){assets.push(Asset{name:a.name.clone(),size:a.size,sha256:digest(a.digest.as_deref())});}}
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
    #[test]fn check_archive_never_executes_or_unpacks(){
        use sha2::{Sha256,Digest};
        let d=tempfile::tempdir().unwrap();
        let name="Remote-EX-v0.1.0-alpha.1-Windows-x64.zip";
        let p=d.path().join(name);let bytes=b"fixture, not executable";std::fs::write(&p,bytes).unwrap();
        let a=Asset{name:name.into(),size:bytes.len()as u64,sha256:Some(remote_ex_signaling::hex(&Sha256::digest(bytes)))};
        assert!(check_archive(&p,&a).unwrap());
        std::fs::write(&p,vec![b'x';bytes.len()]).unwrap();assert!(!check_archive(&p,&a).unwrap());
        std::fs::write(&p,b"short").unwrap();assert!(check_archive(&p,&a).is_err());
        assert!(check_archive(&d.path().join("different.zip"),&a).is_err());
    }
    #[test]fn only_exact_release_links_and_digests(){
        assert_eq!(release_page("v0.1.0-alpha.1").unwrap(),"https://github.com/xzcode/remote-ex/releases/tag/v0.1.0-alpha.1");
        assert!(release_page("v1.0.0/../other").is_err());
        assert_eq!(digest(Some(&format!("sha256:{}","ab".repeat(32)))),Some("ab".repeat(32)));
        assert!(digest(Some("sha256:bad")).is_none());assert!(digest(Some("https://example.org")).is_none());
    }
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
