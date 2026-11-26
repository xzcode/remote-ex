//! Portable platform gates and Wake-on-LAN packet construction.
//! No implicit capture, OS command execution, login or network transmission.
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum LinuxSession{X11,Wayland,Unavailable}
pub fn classify_linux(session:Option<&str>,display:Option<&str>,wayland:Option<&str>,wayland_socket:Option<&str>)->LinuxSession{
    if session==Some("wayland")||wayland.is_some_and(|s|!s.is_empty())||wayland_socket.is_some_and(|s|!s.is_empty()){return LinuxSession::Wayland;}
    if session==Some("x11")&&display.is_some_and(|s|!s.is_empty()){LinuxSession::X11}else{LinuxSession::Unavailable}
}
pub fn linux_session()->LinuxSession{
    classify_linux(std::env::var("XDG_SESSION_TYPE").ok().as_deref(),std::env::var("DISPLAY").ok().as_deref(),
        std::env::var("WAYLAND_DISPLAY").ok().as_deref(),std::env::var("WAYLAND_SOCKET").ok().as_deref())
}
pub fn require_linux_x11()->Result<(),String>{
    match linux_session(){LinuxSession::X11=>Ok(()),LinuxSession::Wayland=>Err("Wayland requires a separately authorized portal session; XWayland fallback is disabled".into()),LinuxSession::Unavailable=>Err("An explicitly logged-in X11 desktop is required".into())}
}
/// Local OS picker source; never supplied by a remote protocol message.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum PortalSource{Monitor,Window,Virtual}
impl PortalSource{
    pub fn parse(s:&str)->Result<Self,String>{match s{"monitor"=>Ok(Self::Monitor),"window"=>Ok(Self::Window),"virtual"=>Ok(Self::Virtual),_=>Err("Unknown local portal source".into())}}
    pub fn as_str(self)->&'static str{match self{Self::Monitor=>"monitor",Self::Window=>"window",Self::Virtual=>"virtual"}}
}
pub fn require_wayland()->Result<(),String>{if linux_session()==LinuxSession::Wayland{Ok(())}else{Err("A native Wayland desktop session is required".into())}}
pub fn wake_packet(mac:&str)->Result<[u8;102],String>{
    let normalized=mac.trim().replace(':',"").replace('-',"");
    if normalized.len()!=12||!normalized.bytes().all(|v|v.is_ascii_hexdigit()){return Err("Enter a six-byte network adapter MAC address".into());}
    let mut address=[0u8;6];
    for i in 0..6{address[i]=u8::from_str_radix(&normalized[i*2..i*2+2],16).map_err(|_|"Invalid MAC address")?;}
    if address==[0;6]||address==[255;6]||address[0]&1!=0{return Err("Choose an individual unicast network adapter address".into());}
    let mut packet=[255;102];
    for chunk in packet[6..].chunks_exact_mut(6){chunk.copy_from_slice(&address);}
    Ok(packet)
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn linux_classification_never_uses_xwayland(){
        assert_eq!(classify_linux(Some("x11"),Some(":0"),None,None),LinuxSession::X11);
        assert_eq!(classify_linux(Some("wayland"),Some(":0"),None,None),LinuxSession::Wayland);
        assert_eq!(classify_linux(Some("x11"),Some(":0"),Some("wayland-0"),None),LinuxSession::Wayland);
        assert_eq!(classify_linux(None,Some(":0"),None,None),LinuxSession::Unavailable);
        assert_eq!(classify_linux(Some("x11"),None,None,None),LinuxSession::Unavailable);
    }
    #[test]fn portal_sources_are_explicit_and_finite(){
        assert_eq!(PortalSource::parse("virtual").unwrap(),PortalSource::Virtual);
        assert_eq!(PortalSource::parse("monitor").unwrap().as_str(),"monitor");
        for s in ["","all","default","/dev/dri/card0","screen:1"]{assert!(PortalSource::parse(s).is_err());}
    }
    #[test]fn wol_requires_unicast_and_exact_magic_packet(){
        let a=[0x02,0x12,0x34,0x56,0x78,0x9a];
        let p=wake_packet("02:12:34:56:78:9A").unwrap();
        assert_eq!(&p[..6],&[255;6]);for chunk in p[6..].chunks_exact(6){assert_eq!(chunk,&a);}
        assert_eq!(wake_packet("02-12-34-56-78-9a").unwrap(),p);
        for bad in ["ff:ff:ff:ff:ff:ff","00:00:00:00:00:00","01:00:00:00:00:01","invalid","02:12:34"]{assert!(wake_packet(bad).is_err());}
    }
}
