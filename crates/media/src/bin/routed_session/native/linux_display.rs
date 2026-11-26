//! X11 input is confined to one approved, local, unlocked Xorg desktop.
//! No XWayland, remote DISPLAY, elevation, or lock-screen fallback. The
//! production owner still requires authenticated transport and local consent.
use super::*;
use std::sync::Mutex;
use x11rb::{connection::Connection as _,rust_connection::RustConnection,
    protocol::{xproto::{self,ConnectionExt as _,AtomEnum},xtest::ConnectionExt as _}};
static SESSION_CHECK:Mutex<Option<Instant>>=Mutex::new(None);

fn active_local_x11(text:&str,display:&str)->bool{
    let mut active=false;let mut unlocked=false;let mut local=false;let mut kind=false;let mut user=false;let mut matching=false;
    let normalized=display.strip_suffix(".0").unwrap_or(display);
    for line in text.lines(){match line{
        "Active=yes"=>active=true,"LockedHint=no"=>unlocked=true,"Remote=no"=>local=true,
        "Type=x11"=>kind=true,"Class=user"=>user=true,
        _=>if let Some(v)=line.strip_prefix("Display="){matching=v==normalized;},
    }}
    active&&unlocked&&local&&kind&&user&&matching
}
fn local_display()->MediaResult<String>{
    remote_ex_desktop_features::platform::require_linux_x11()?;
    let s=std::env::var("DISPLAY").map_err(|_|"Missing local X11 display")?;
    let raw=s.strip_prefix(':').ok_or("Nonlocal X11 display refused")?;
    let n=raw.strip_suffix(".0").unwrap_or(raw);
    if n.is_empty()||n.len()>5||!n.bytes().all(|b|b.is_ascii_digit()){return Err("Only a local X11 screen-zero display is supported".into());}
    Ok(s)
}
pub(super) fn require_local_session()->MediaResult<()>{
    let display=local_display()?;
    let mut cache=SESSION_CHECK.lock().map_err(|_|"X11 session check unavailable")?;
    if cache.is_some_and(|t|t.elapsed()<Duration::from_millis(500)){return Ok(());}
    *cache=None;
    let id=std::env::var("XDG_SESSION_ID").map_err(|_|"X11 control requires a logind user session")?;
    if id.is_empty()||id.len()>32||!id.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-'||b==b'_'){return Err("Invalid local session ID".into());}
    // Fixed read-only system query, no shell, no remote arguments. Bounded
    // wait avoids hanging the capture worker on an unhealthy session bus.
    use std::process::{Command,Stdio};
    let mut child=Command::new("/usr/bin/loginctl")
        .args(["show-session",&id,"--property=Active","--property=LockedHint","--property=Remote","--property=Type","--property=Class","--property=Display","--no-pager"])
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|_|"Cannot verify logind session")?;
    let deadline=Instant::now()+Duration::from_millis(750);
    let status=loop{
        match child.try_wait(){
            Ok(Some(s))=>break s,Ok(None) if Instant::now()<deadline=>std::thread::sleep(Duration::from_millis(10)),
            _=>{let _=child.kill();let _=child.wait();return Err("Local session query timed out".into());}
        }
    };
    let mut data=String::new();
    child.stdout.take().ok_or("Missing session query output")?.take(2049).read_to_string(&mut data).map_err(|_|"Cannot inspect local session")?;
    if !status.success()||data.len()>2048||!active_local_x11(&data,&display){return Err("X11 input requires an active, unlocked, local user session matching DISPLAY".into());}
    *cache=Some(Instant::now());Ok(())
}
fn physical(usage:u16)->Option<u8>{
    // Linux evdev + 8 Xorg keycodes, only with verified evdev XKB rules.
    const LETTERS:[u8;26]=[30,48,46,32,18,33,34,35,23,36,37,38,50,49,24,25,16,19,31,20,22,47,17,45,21,44];
    let evdev=match usage{
        4..=29=>LETTERS[(usage-4)as usize],30..=39=>(usage-28)as u8,
        40=>28,41=>1,42=>14,43=>15,44=>57,45=>12,46=>13,47=>26,48=>27,49=>43,
        51=>39,52=>40,53=>41,54=>51,55=>52,56=>53,57=>58,
        58..=67=>(usage-58+59)as u8,68=>87,69=>88,
        73=>110,74=>102,75=>104,76=>111,77=>107,78=>109,79=>106,80=>105,81=>108,82=>103,
        224=>29,225=>42,226=>56,227=>125,228=>97,229=>54,230=>100,231=>126,_=>return None,
    };evdev.checked_add(8)
}
fn button(id:u8)->InputResult<u8>{match id{1..=3=>Ok(id),_=>Err("X11 candidate supports left, middle, right buttons".into())}}
struct XInput{conn:RustConnection,root:u32,area:DisplayArea}
impl XInput{
    fn open(area:DisplayArea)->MediaResult<Self>{
        let display=local_display()?;
        let(conn,index)=x11rb::connect(Some(&display)).map_err(error)?;
        if conn.setup().roots.len()!=1||index!=0{return Err("X11 control requires one X screen".into());}
        let root=conn.setup().roots[index].root;
        let v=conn.xtest_get_version(2,2).map_err(error)?.reply().map_err(error)?;
        if v.major_version<2{return Err("XTEST unavailable".into());}
        let backend=Self{conn,root,area};backend.verify_geometry()?;backend.verify_keymap()?;Ok(backend)
    }
    fn verify_geometry(&self)->InputResult<()>{
        let g=self.conn.get_geometry(self.root).map_err(error)?.reply().map_err(error)?;
        if self.area.x!=0||self.area.y!=0||u32::from(g.width)!=self.area.width||u32::from(g.height)!=self.area.height||self.area.width>32767||self.area.height>32767{return Err("X11 root geometry differs from approved display".into());}Ok(())
    }
    fn verify_keymap(&self)->InputResult<()>{
        let atom=self.conn.intern_atom(true,b"_XKB_RULES_NAMES").map_err(error)?.reply().map_err(error)?.atom;
        if atom==0{return Err("XKB rules unavailable".into());}
        let r=self.conn.get_property(false,self.root,atom,AtomEnum::STRING,0,256).map_err(error)?.reply().map_err(error)?;
        if r.bytes_after!=0||r.value.split(|b|*b==0).next()!=Some(b"evdev".as_slice()){return Err("Unsupported XKB physical key mapping (evdev required)".into());}
        Ok(())
    }
    fn fake(&self,event:u8,detail:u8,x:i16,y:i16)->InputResult<()>{
        self.conn.xtest_fake_input(event,detail,x11rb::CURRENT_TIME,self.root,x,y,0).map_err(error)?.check().map_err(error)?;
        self.conn.flush().map_err(error)
    }
    fn pointer(&self)->InputResult<(i32,i32)>{
        let p=self.conn.query_pointer(self.root).map_err(error)?.reply().map_err(error)?;
        if !p.same_screen{return Err("Pointer is on another X screen".into());}
        Ok((i32::from(p.root_x),i32::from(p.root_y)))
    }
    fn contains(&self,x:i32,y:i32)->bool{x>=0&&y>=0&&x<(self.area.width as i32)&&y<(self.area.height as i32)}
    fn require_pointer(&self)->InputResult<()>{
        let(x,y)=self.pointer()?;if !self.contains(x,y){return Err("Pointer outside approved X11 display".into());}Ok(())
    }
    fn require_focus(&self)->InputResult<()>{
        let focus=self.conn.get_input_focus().map_err(error)?.reply().map_err(error)?.focus;
        if focus<=1||focus==self.root{return Err("No focused X11 application window".into());}
        let attrs=self.conn.get_window_attributes(focus).map_err(error)?.reply().map_err(error)?;
        let geo=self.conn.get_geometry(focus).map_err(error)?.reply().map_err(error)?;
        let t=self.conn.translate_coordinates(focus,self.root,0,0).map_err(error)?.reply().map_err(error)?;
        let(x,y)=(i32::from(t.dst_x),i32::from(t.dst_y));
        if attrs.map_state!=xproto::MapState::VIEWABLE||!t.same_screen||geo.root!=self.root||!self.contains(x,y)||i64::from(x)+i64::from(geo.width)>i64::from(self.area.width)||i64::from(y)+i64::from(geo.height)>i64::from(self.area.height){return Err("Focused window outside the approved X11 display".into());}
        Ok(())
    }
    fn move_to(&self,x:i32,y:i32)->InputResult<()>{
        if !self.contains(x,y){return Err("Pointer outside approved X11 display".into());}
        self.fake(xproto::MOTION_NOTIFY_EVENT,0,x as i16,y as i16)?;
        if self.pointer()?!=(x,y){return Err("XTEST cursor position mismatch".into());}Ok(())
    }
    fn key(&self,usage:u16,down:bool)->InputResult<()>{
        let code=physical(usage).ok_or("Unsupported HID key for X11")?;
        if down{self.require_pointer()?;self.require_focus()?;self.verify_keymap()?;}
        self.fake(if down{xproto::KEY_PRESS_EVENT}else{xproto::KEY_RELEASE_EVENT},code,0,0)
    }
    fn button(&self,id:u8,down:bool)->InputResult<()>{
        if down{self.require_pointer()?;}
        self.fake(if down{xproto::BUTTON_PRESS_EVENT}else{xproto::BUTTON_RELEASE_EVENT},button(id)?,0,0)
    }
}
pub(super) struct ScopedBackend{input:XInput,display:u32}
impl ScopedBackend{
    pub(super) fn new(display:u32,area:DisplayArea)->MediaResult<Self>{require_local_session()?;Ok(Self{input:XInput::open(area)?,display})}
}
impl Backend for ScopedBackend{
    fn check_display(&mut self,area:DisplayArea)->InputResult<()>{
        require_local_session()?;
        if self.input.area!=area{return Err("Approved display changed".into());}
        monitor(self.display,Some(area),true,false)?;self.input.verify_geometry()
    }
    fn move_to(&mut self,x:i32,y:i32)->InputResult<()>{self.input.move_to(x,y)}
    fn position(&mut self)->InputResult<(i32,i32)>{self.input.pointer()}
    fn key(&mut self,usage:u16,down:bool)->InputResult<()>{self.input.key(usage,down)}
    fn button(&mut self,id:u8,down:bool)->InputResult<()>{self.input.button(id,down)}
    fn scroll_steps(&mut self,x:i32,y:i32)->InputResult<()>{
        self.input.require_pointer()?;
        if x.unsigned_abs()>20||y.unsigned_abs()>20{return Err("X11 scroll step limit exceeded".into());}
        for (n,up,down) in [(y,4,5),(x,6,7)]{let b=if n<0{up}else{down};
            for _ in 0..n.unsigned_abs(){self.input.fake(xproto::BUTTON_PRESS_EVENT,b,0,0)?;self.input.fake(xproto::BUTTON_RELEASE_EVENT,b,0,0)?;}
        }Ok(())
    }
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn session_state_and_physical_keys(){
        let s="Active=yes\nLockedHint=no\nRemote=no\nType=x11\nClass=user\nDisplay=:0\n";
        assert!(active_local_x11(s,":0.0"));assert!(!active_local_x11(s,":1"));
        assert!(!active_local_x11(&s.replace("LockedHint=no","LockedHint=yes"),":0"));
        assert!(!active_local_x11(&s.replace("Type=x11","Type=wayland"),":0"));
        assert!(!active_local_x11(&s.replace("Active=yes","Active=no"),":0"));
        assert_eq!(physical(4),Some(38));assert_eq!(physical(39),Some(19));assert!(physical(0).is_none());assert!(button(4).is_err());
    }
    #[test]fn actual_xtest_on_disposable_fixture(){
        if std::env::var("REMOTE_EX_X11_FIXTURE").as_deref()!=Ok("1")||std::env::var("RUNNER_ENVIRONMENT").as_deref()!=Ok("github-hosted"){return;}
        // Only CI-created display, test-owned window; no production bypass.
        let a=DisplayArea{x:0,y:0,width:1024,height:768};
        let x=XInput::open(a).unwrap();
        let window=x.conn.generate_id().unwrap();
        x.conn.create_window(x11rb::COPY_DEPTH_FROM_PARENT,window,x.root,10,10,400,300,0,xproto::WindowClass::INPUT_OUTPUT,0,&xproto::CreateWindowAux::new().event_mask(xproto::EventMask::KEY_PRESS|xproto::EventMask::KEY_RELEASE)).unwrap().check().unwrap();
        x.conn.map_window(window).unwrap().check().unwrap();
        x.conn.set_input_focus(xproto::InputFocus::PARENT,window,x11rb::CURRENT_TIME).unwrap().check().unwrap();
        x.move_to(100,100).unwrap();assert_eq!(x.pointer().unwrap(),(100,100));assert!(x.move_to(1024,100).is_err());
        x.key(4,true).unwrap();let code=physical(4).unwrap()as usize;
        let keys=x.conn.query_keymap().unwrap().reply().unwrap().keys;assert_ne!(keys[code/8]&(1<<(code%8)),0);
        x.key(4,false).unwrap();let keys=x.conn.query_keymap().unwrap().reply().unwrap().keys;assert_eq!(keys[code/8]&(1<<(code%8)),0);
        x.button(1,true).unwrap();x.button(1,false).unwrap();
        x.conn.destroy_window(window).unwrap().check().unwrap();
    }
}
