//! Explicitly approved single-display Quartz input for macOS.
//! XCap 0.9.10 enumerates CGDirectDisplayID and CGDisplayBounds (global
//! logical points). Captured pixels may be higher resolution on Retina.
//! Enigo 0.6.1's mouse location mixes NSEvent and main-display dimensions;
//! use Quartz events/positions instead. Reuse Enigo only for physical keys.
//! No OS permission bypass, private API, capture of extra displays or
//! automated permission prompts. Focus may race local OS changes.
use super::*;
use std::{ffi::{c_char,c_void},ptr};

#[repr(C)]
#[derive(Clone,Copy,Debug,Default)]
struct Point{x:f64,y:f64}
#[repr(C)]
#[derive(Clone,Copy,Debug,Default)]
struct Size{width:f64,height:f64}
#[repr(C)]
#[derive(Clone,Copy)]
struct Bounds{origin:Point,size:Size}
type Ref=*const c_void;

#[link(name="CoreGraphics",kind="framework")]
extern "C"{
    fn CGDisplayBounds(display:u32)->Bounds;
    fn CGDisplayIsActive(display:u32)->u32;
    fn CGDisplayIsInMirrorSet(display:u32)->u32;
    fn CGGetDisplaysWithPoint(point:Point,max:u32,displays:*mut u32,count:*mut u32)->i32;
    fn CGEventCreate(source:Ref)->*mut c_void;
    fn CGEventGetLocation(event:Ref)->Point;
    fn CGEventCreateMouseEvent(source:Ref,kind:u32,point:Point,button:u32)->*mut c_void;
    fn CGEventCreateScrollWheelEvent(source:Ref,units:u32,wheels:u32,wheel1:i32,...)->*mut c_void;
    fn CGEventSetLocation(event:*mut c_void,point:Point);
    fn CGEventSetIntegerValueField(event:*mut c_void,field:u32,value:i64);
    fn CGEventSetFlags(event:*mut c_void,flags:u64);
    fn CGEventSourceFlagsState(state:i32)->u64;
    fn CGEventPost(tap:u32,event:Ref);
}
#[link(name="CoreFoundation",kind="framework")]
extern "C"{
    fn CFRelease(value:Ref);
    fn CFGetTypeID(value:Ref)->usize;
    fn CFStringCreateWithCString(allocator:Ref,text:*const c_char,encoding:u32)->Ref;
}
#[link(name="ApplicationServices",kind="framework")]
extern "C"{
    fn AXUIElementCreateSystemWide()->Ref;
    fn AXUIElementGetTypeID()->usize;
    fn AXUIElementSetMessagingTimeout(element:Ref,seconds:f32)->i32;
    fn AXUIElementCopyAttributeValue(element:Ref,attribute:Ref,value:*mut Ref)->i32;
    fn AXValueGetTypeID()->usize;
    fn AXValueGetValue(value:Ref,kind:u32,out:*mut c_void)->u8;
}
struct Owned(Ref);
impl Owned{
    fn new(value:Ref)->MediaResult<Self>{if value.is_null(){Err("Quartz or accessibility object unavailable".into())}else{Ok(Self(value))}}
    fn mutable(&self)->*mut c_void{self.0 as *mut c_void}
}
impl Drop for Owned{fn drop(&mut self){unsafe{CFRelease(self.0);}}}
fn contains(area:DisplayArea,p:Point)->bool{
    p.x.is_finite()&&p.y.is_finite()&&p.x>=f64::from(area.x)&&p.y>=f64::from(area.y)
        &&p.x<f64::from(area.x)+f64::from(area.width)&&p.y<f64::from(area.y)+f64::from(area.height)
}
fn window_inside(area:DisplayArea,p:Point,s:Size)->bool{
    contains(area,p)&&s.width.is_finite()&&s.height.is_finite()&&s.width>0.0&&s.height>0.0
        &&p.x+s.width<=f64::from(area.x)+f64::from(area.width)
        &&p.y+s.height<=f64::from(area.y)+f64::from(area.height)
}
fn selected_point(display:u32,p:Point)->MediaResult<()>{
    let mut ids=[0u32;2];let mut n=0u32;
    // SAFETY: fixed two-element array and valid count output.
    if unsafe{CGGetDisplaysWithPoint(p,2,ids.as_mut_ptr(),&mut n)}!=0||n!=1||ids[0]!=display{
        return Err("Point is not on the unique approved macOS display".into());
    }Ok(())
}
pub(super) fn verify_area(display:u32,area:DisplayArea)->MediaResult<()>{
    area.validate()?;
    // SAFETY: scalar display identifiers, no pointers or ownership transfer.
    if unsafe{CGDisplayIsActive(display)}==0||unsafe{CGDisplayIsInMirrorSet(display)}!=0{
        return Err("Inactive or mirrored macOS display".into());
    }
    let b=unsafe{CGDisplayBounds(display)};
    if b.origin.x!=f64::from(area.x)||b.origin.y!=f64::from(area.y)
        ||b.size.width!=f64::from(area.width)||b.size.height!=f64::from(area.height){
        return Err("Quartz and XCap display geometry disagree".into());
    }
    let (x,y)=area.map(32768,32768)?;
    selected_point(display,Point{x:f64::from(x),y:f64::from(y)})
}
fn cursor()->MediaResult<Point>{
    // SAFETY: Create returns an owned CF object or NULL; no event is posted.
    let event=Owned::new(unsafe{CGEventCreate(ptr::null())})?;
    let p=unsafe{CGEventGetLocation(event.0)};
    if !p.x.is_finite()||!p.y.is_finite()||p.x<i32::MIN as f64||p.x>i32::MAX as f64||p.y<i32::MIN as f64||p.y>i32::MAX as f64{
        return Err("Invalid Quartz cursor position".into());
    }Ok(p)
}
fn attribute(element:&Owned,name:&[u8])->MediaResult<Owned>{
    // Every caller supplies a fixed NUL-terminated ASCII attribute name.
    if name.last()!=Some(&0){return Err("Invalid accessibility attribute".into());}
    let attr=Owned::new(unsafe{CFStringCreateWithCString(ptr::null(),name.as_ptr().cast(),0x08000100)})?;
    let mut value:Ref=ptr::null();
    if unsafe{AXUIElementCopyAttributeValue(element.0,attr.0,&mut value)}!=0{
        return Err("Cannot verify focused macOS window".into());
    }
    Owned::new(value)
}
fn require_element(value:&Owned)->MediaResult<()>{
    if unsafe{CFGetTypeID(value.0)}!=unsafe{AXUIElementGetTypeID()}{
        return Err("Unexpected accessibility element".into());
    }
    if unsafe{AXUIElementSetMessagingTimeout(value.0,0.25)}!=0{
        return Err("Cannot bound accessibility query".into());
    }Ok(())
}
fn focused_window_inside(area:DisplayArea)->MediaResult<()>{
    let system=Owned::new(unsafe{AXUIElementCreateSystemWide()})?;require_element(&system)?;
    let app=attribute(&system,b"AXFocusedApplication\0")?;require_element(&app)?;
    let window=attribute(&app,b"AXFocusedWindow\0")?;require_element(&window)?;
    let pos=attribute(&window,b"AXPosition\0")?;
    let size=attribute(&window,b"AXSize\0")?;
    let value_type=unsafe{AXValueGetTypeID()};
    if unsafe{CFGetTypeID(pos.0)}!=value_type||unsafe{CFGetTypeID(size.0)}!=value_type{
        return Err("Focused window has no valid geometry".into());
    }
    let mut p=Point::default();let mut s=Size::default();
    // SAFETY: AXValue types CGPoint=1, CGSize=2; correct C-compatible storage.
    if unsafe{AXValueGetValue(pos.0,1,(&mut p as *mut Point).cast())}==0
        ||unsafe{AXValueGetValue(size.0,2,(&mut s as *mut Size).cast())}==0||!window_inside(area,p,s){
        return Err("Focused window is not entirely on the approved macOS display".into());
    }Ok(())
}
fn post_mouse(kind:u32,p:Point,button:u32,click:bool)->MediaResult<()>{
    let e=Owned::new(unsafe{CGEventCreateMouseEvent(ptr::null(),kind,p,button)})?;
    // SAFETY: valid created event retained until posting. Field 1 is click state.
    unsafe{
        if click{CGEventSetIntegerValueField(e.mutable(),1,1);}
        CGEventSetFlags(e.mutable(),CGEventSourceFlagsState(0));
        CGEventPost(0,e.0);
    }Ok(())
}
fn button_event(id:u8,down:bool)->MediaResult<(u32,u32)>{
    Ok(match (id,down){
        (1,true)=>(1,0),(1,false)=>(2,0),
        (2,true)=>(25,2),(2,false)=>(26,2),
        (3,true)=>(3,1),(3,false)=>(4,1),
        _=>return Err("macOS scoped control supports left, middle and right mouse buttons".into()),
    })
}
pub(super) struct ScopedBackend{inner:NativeBackend,display:u32,area:DisplayArea,held:[bool;3]}
impl ScopedBackend{
    pub(super) fn new(display:u32,area:DisplayArea)->MediaResult<Self>{
        verify_area(display,area)?;
        Ok(Self{inner:NativeBackend::new()?,display,area,held:[false;3]})
    }
    fn require_position(&self)->MediaResult<Point>{
        let p=cursor()?;
        if !contains(self.area,p){return Err("Pointer outside approved macOS display".into());}
        selected_point(self.display,p)?;Ok(p)
    }
}
impl Backend for ScopedBackend{
    fn check_display(&mut self,area:DisplayArea)->InputResult<()>{
        if area!=self.area{return Err("Approved display changed".into());}
        monitor(self.display,Some(area),true,true)?;Ok(())
    }
    fn move_to(&mut self,x:i32,y:i32)->InputResult<()>{
        let p=Point{x:f64::from(x),y:f64::from(y)};
        if !contains(self.area,p){return Err("Pointer outside approved macOS display".into());}
        selected_point(self.display,p)?;
        let(kind,button)=if self.held[0]{(6,0)}else if self.held[2]{(7,1)}else if self.held[1]{(27,2)}else{(5,0)};
        post_mouse(kind,p,button,false)?;
        let until=Instant::now()+Duration::from_millis(50);
        loop{
            let actual=cursor()?;
            if actual.x.floor()==p.x&&actual.y.floor()==p.y{
                return selected_point(self.display,actual);
            }
            if canceled()||Instant::now()>=until{return Err("Quartz cursor did not reach the approved point".into());}
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    fn position(&mut self)->InputResult<(i32,i32)>{
        let p=cursor()?;Ok((p.x.floor() as i32,p.y.floor() as i32))
    }
    fn key(&mut self,usage:u16,down:bool)->InputResult<()>{
        if down{self.require_position()?;focused_window_inside(self.area)?;}
        // Releases must remain possible after focus changes.
        self.inner.key(usage,down)
    }
    fn button(&mut self,id:u8,down:bool)->InputResult<()>{
        let(kind,button)=button_event(id,down)?;
        let index=usize::from(id-1);
        if !down&&!self.held[index]{return Ok(());}
        let point=if down{self.require_position()?}else{cursor()?};
        if down{self.held[index]=true;}
        post_mouse(kind,point,button,true)?;
        if !down{self.held[index]=false;}
        Ok(())
    }
    fn scroll_steps(&mut self,x:i32,y:i32)->InputResult<()>{
        let p=self.require_position()?;
        // SAFETY: wheel count 2, second variadic argument is C int, line units.
        let e=Owned::new(unsafe{CGEventCreateScrollWheelEvent(ptr::null(),1,2,y.saturating_neg(),x.saturating_neg())})?;
        unsafe{CGEventSetLocation(e.mutable(),p);CGEventSetFlags(e.mutable(),CGEventSourceFlagsState(0));CGEventPost(0,e.0);}
        Ok(())
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn negative_origin_and_focused_window_bounds(){
        let a=DisplayArea{x:-1920,y:-240,width:1920,height:1080};
        assert!(contains(a,Point{x:-1.0,y:839.0}));
        assert!(!contains(a,Point{x:0.0,y:0.0}));
        assert!(window_inside(a,Point{x:-1800.0,y:-100.0},Size{width:1000.0,height:700.0}));
        assert!(!window_inside(a,Point{x:-100.0,y:0.0},Size{width:200.0,height:300.0}));
        assert!(!window_inside(a,Point{x:-100.0,y:0.0},Size{width:f64::NAN,height:300.0}));
    }
    #[test]fn button_mapping_is_explicit(){
        assert_eq!(button_event(1,true).unwrap(),(1,0));
        assert_eq!(button_event(3,false).unwrap(),(4,1));
        assert_eq!(button_event(2,true).unwrap(),(25,2));
        assert!(button_event(4,true).is_err());
    }
}
