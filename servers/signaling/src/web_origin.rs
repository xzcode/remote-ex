//! Exact opt-in browser Origin gate. Native clients without an Origin
//! header remain supported. Origin is not an authentication credential;
//! every WebSocket still requires the normal TLS and enrolled-key proof.
use axum::http::{HeaderMap,header::ORIGIN};
use url::{Url,Host};
#[derive(Clone,Debug)]
pub struct WebOrigin(String);
impl WebOrigin{
    pub fn parse(value:&str,local_only:bool)->Result<Self,&'static str>{
        if value.len()>512||value.chars().any(|c|c.is_control()||c.is_whitespace()){return Err("Invalid Web origin");}
        let u=Url::parse(value).map_err(|_|"Invalid Web origin")?;
        if u.host().is_none()||!u.username().is_empty()||u.password().is_some()||u.path()!="/"||u.query().is_some()||u.fragment().is_some(){return Err("Web origin must be an exact origin, without path or credentials");}
        let local=local_only&&u.scheme()=="http"&&matches!(u.host(),Some(Host::Ipv4(ip)) if ip.is_loopback());
        if u.scheme()!="https"&&!local{return Err("Web origin requires HTTPS (explicit local-only HTTP loopback fixture allowed)");}
        let origin=u.origin().ascii_serialization();
        if origin!=value{return Err("Use the canonical exact Web origin without trailing slash");}
        Ok(Self(origin))
    }
}
pub(crate) fn allowed(headers:&HeaderMap,configured:Option<&WebOrigin>)->bool{
    let all=headers.get_all(ORIGIN);
    let mut values=all.iter();
    let Some(value)=values.next()else{return true;};
    if values.next().is_some(){return false;}
    configured.is_some_and(|origin|value.to_str().is_ok_and(|v|v==origin.0.as_str()))
}
#[cfg(test)]mod tests{
    use super::*;
    use axum::http::HeaderValue;
    #[test]fn exact_origin_and_native_no_origin(){
        let mut headers=HeaderMap::new();
        let o=WebOrigin::parse("https://remote.example.net",false).unwrap();
        assert!(allowed(&headers,None));assert!(allowed(&headers,Some(&o)));
        headers.insert(ORIGIN,HeaderValue::from_static("https://remote.example.net"));
        assert!(!allowed(&headers,None));assert!(allowed(&headers,Some(&o)));
        headers.append(ORIGIN,HeaderValue::from_static("https://remote.example.net"));assert!(!allowed(&headers,Some(&o)));
        for v in ["null","https://remote.example.net.evil","https://remote.example.net:444","https://remote.example.net/"]{headers.clear();headers.insert(ORIGIN,HeaderValue::from_str(v).unwrap());assert!(!allowed(&headers,Some(&o)));}
        assert!(WebOrigin::parse("http://127.0.0.1:8080",true).is_ok());assert!(WebOrigin::parse("http://127.0.0.1:8080",false).is_err());
        for v in ["*","null","http://remote.example.net","https://remote.example.net/path","https://user@remote.example.net","https://remote.example.net/","https://remote.example.net?x=1"]{assert!(WebOrigin::parse(v,true).is_err());}
    }
}
