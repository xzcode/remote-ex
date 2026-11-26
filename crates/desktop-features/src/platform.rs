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
/// Never select the default PipeWire source. Modern portals must provide
/// an exact stable serial; legacy nodes are valid only inside their portal fd.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum PortalTarget{Node(u32),Serial(u64)}
pub fn portal_target(version:u32,node:u32,serial:Option<u64>)->Result<PortalTarget,String>{
    if version<2||node==0||node==u32::MAX{return Err("Invalid ScreenCast portal stream".into());}
    if let Some(s)=serial.filter(|s|*s!=0&&*s!=u64::MAX){return Ok(PortalTarget::Serial(s));}
    if version>=6{return Err("ScreenCast portal v6+ must supply a stable PipeWire serial".into());}
    Ok(PortalTarget::Node(node))
}
pub fn require_wayland()->Result<(),String>{if linux_session()==LinuxSession::Wayland{Ok(())}else{Err("A native Wayland desktop session is required".into())}}
/// Fail closed after suspend, prolonged supervisor starvation or a wall
/// clock rollback; a new local approval is required instead of auto-resume.
pub fn lifecycle_gap(mono:std::time::Duration,wall:Result<std::time::Duration,std::time::SystemTimeError>)->bool{
    let ceiling=std::time::Duration::from_secs(6);
    mono<=ceiling&&wall.is_ok_and(|d|d<=ceiling)
}
pub struct LifecycleWatch{mono:std::time::Instant,wall:std::time::SystemTime}
impl Default for LifecycleWatch{fn default()->Self{Self{mono:std::time::Instant::now(),wall:std::time::SystemTime::now()}}}
impl LifecycleWatch{
    pub fn check(&mut self)->Result<(),String>{
        let mono=std::time::Instant::now();let wall=std::time::SystemTime::now();
        let ok=lifecycle_gap(mono.saturating_duration_since(self.mono),wall.duration_since(self.wall));self.mono=mono;self.wall=wall;
        if !ok{return Err("Local session suspended or clock changed; authorize again".into());}Ok(())
    }
}
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
    #[test]fn lifecycle_gap_stops_instead_of_resuming(){
        use std::time::{Duration,SystemTime};
        assert!(lifecycle_gap(Duration::from_millis(500),Ok(Duration::from_millis(500))));
        assert!(!lifecycle_gap(Duration::from_millis(500),Ok(Duration::from_secs(30))));
        assert!(!lifecycle_gap(Duration::from_secs(30),Ok(Duration::from_millis(500))));
        assert!(!lifecycle_gap(Duration::from_millis(500),SystemTime::UNIX_EPOCH.duration_since(SystemTime::UNIX_EPOCH+Duration::from_secs(1))));
    }
    #[test]fn exact_portal_target_never_falls_back(){
        assert_eq!(portal_target(5,7,None).unwrap(),PortalTarget::Node(7));
        assert_eq!(portal_target(6,7,Some(4294967300)).unwrap(),PortalTarget::Serial(4294967300));
        assert!(portal_target(6,7,None).is_err());assert!(portal_target(6,7,Some(0)).is_err());
        assert!(portal_target(5,0,None).is_err());assert!(portal_target(1,7,Some(3)).is_err());
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
