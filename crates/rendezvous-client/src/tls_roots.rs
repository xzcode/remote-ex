//! Exact same root selection for preflight, presence, CLI and routed media.
//! A custom certificate-only file REPLACES public roots; no silent fallback,
//! no private keys, no certificate/hostname validation bypass.
use crate::Result;
use std::{io::{BufReader,Read},path::Path};
const MAX:usize=256*1024;
pub fn public()->rustls::RootCertStore{rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned())}
pub fn private(path:&str)->Result<rustls::RootCertStore>{
    if path.is_empty()||path.len()>2048||!Path::new(path).is_absolute(){return Err("Choose an absolute local CA certificate path".into());}
    let path=Path::new(path);
    let meta=std::fs::symlink_metadata(path).map_err(|_|"Cannot inspect private CA file")?;
    if !meta.file_type().is_file()||meta.len()==0||meta.len()>MAX as u64{return Err("Invalid private CA file type or size".into());}
    let file=std::fs::File::open(path).map_err(|_|"Cannot open private CA file")?;
    let mut data=Vec::new();
    file.take(MAX as u64+1).read_to_end(&mut data).map_err(|_|"Cannot read private CA file")?;
    parse(&data)
}
fn parse(data:&[u8])->Result<rustls::RootCertStore>{
    if data.is_empty()||data.len()>MAX{return Err("CA file exceeds size bounds".into());}
    let text=std::str::from_utf8(data).map_err(|_|"CA file must contain certificate-only PEM")?;
    // Reject unrecognized PEM blocks as well as keys. rustls_pemfile otherwise
    // may skip unknown sections, so inspect BEGIN/END markers first.
    let mut inside=false;let mut count=0usize;
    for line in text.lines(){
        let line=line.trim();
        if line=="-----BEGIN CERTIFICATE-----"{
            if inside{return Err("Malformed certificate PEM".into());}inside=true;count+=1;
            if count>32{return Err("Too many CA certificates".into());}
        }else if line=="-----END CERTIFICATE-----"{
            if !inside{return Err("Malformed certificate PEM".into());}inside=false;
        }else if line.is_empty(){}else if !inside||line.starts_with("-----")||!line.bytes().all(|b|b.is_ascii_alphanumeric()||matches!(b,b'+'|b'/'|b'=')){
            return Err("Choose certificate-only PEM without private keys or other data".into());
        }
    }
    if inside||count==0{return Err("Missing or incomplete CA certificates".into());}
    let mut roots=rustls::RootCertStore::empty();
    let mut reader=BufReader::new(data);
    for item in rustls_pemfile::read_all(&mut reader){
        match item.map_err(|_|"Invalid CA PEM")?{
            rustls_pemfile::Item::X509Certificate(cert)=>roots.add(cert).map_err(|_|"Invalid CA certificate")?,
            _=>return Err("Choose a certificate-only PEM".into()),
        }
    }
    if roots.len()!=count{return Err("CA certificate count mismatch".into());}Ok(roots)
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn malformed_and_mixed_pem_never_fall_back(){
        for data in [b"".as_slice(),b"-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----",b"-----BEGIN CERTIFICATE-----\nAAAA",b"-----BEGIN SOMETHING-----\nAAAA\n-----END SOMETHING-----",b"-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----"]{assert!(parse(data).is_err());}
        assert!(private("relative.pem").is_err());assert!(!public().is_empty());
    }
}
