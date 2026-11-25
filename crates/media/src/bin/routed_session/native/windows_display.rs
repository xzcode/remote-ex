//! Experimental single-display Windows adapter. No elevation or bypass:
//! ordinary desktop APIs plus Enigo. Physical DPI-aware geometry and the
//! current actual monitor are independently checked before input.
use super::*;
use windows_sys::Win32::{
    Foundation::{POINT,RECT},
    Graphics::Gdi::{GetMonitorInfoW,MonitorFromPoint,MonitorFromWindow,MONITORINFO,MONITOR_DEFAULTTONULL},
    UI::{HiDpi::{SetThreadDpiAwarenessContext,DPI_AWARENESS_CONTEXT,DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2},
         WindowsAndMessaging::{GetCursorPos,SetCursorPos,GetForegroundWindow}},
};
pub(super) struct DpiGuard(DPI_AWARENESS_CONTEXT);
impl DpiGuard{
    pub(super) fn new()->MediaResult<Self>{
        // SAFETY: scoped thread setting, restored by Drop on the same thread.
        let previous=unsafe{SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)};
        if previous.is_null(){return Err("Cannot verify per-monitor DPI context".into());}Ok(Self(previous))
    }
}
impl Drop for DpiGuard{fn drop(&mut self){unsafe{SetThreadDpiAwarenessContext(self.0);}}}
fn contains(area:DisplayArea,x:i32,y:i32)->bool{
    i64::from(x)>=i64::from(area.x)&&i64::from(x)<i64::from(area.x)+i64::from(area.width)&&
    i64::from(y)>=i64::from(area.y)&&i64::from(y)<i64::from(area.y)+i64::from(area.height)
}
pub(super) fn verify_area(area:DisplayArea)->MediaResult<()>{
    let area=area.validate()?;
    let _dpi=DpiGuard::new()?;
    let(x,y)=area.map(32768,32768)?;
    // SAFETY: ordinary Win32 display query; stack storage has correct size.
    let monitor=unsafe{MonitorFromPoint(POINT{x,y},MONITOR_DEFAULTTONULL)};
    if monitor.is_null(){return Err("Authorized Windows monitor unavailable".into());}
    let mut info=MONITORINFO{cbSize:std::mem::size_of::<MONITORINFO>()as u32,rcMonitor:RECT{left:0,top:0,right:0,bottom:0},rcWork:RECT{left:0,top:0,right:0,bottom:0},dwFlags:0};
    if unsafe{GetMonitorInfoW(monitor,&mut info)}==0{return Err("Cannot query Windows monitor geometry".into());}
    let r=info.rcMonitor;
    if r.left!=area.x||r.top!=area.y||i64::from(r.right)-i64::from(r.left)!=i64::from(area.width)||i64::from(r.bottom)-i64::from(r.top)!=i64::from(area.height){
        return Err("XCap and physical Windows display geometry disagree".into());
    }
    Ok(())
}
pub(super) struct ScopedBackend{inner:NativeBackend,display:u32,area:DisplayArea}
impl ScopedBackend{
    pub(super) fn new(display:u32,area:DisplayArea)->MediaResult<Self>{verify_area(area)?;Ok(Self{inner:NativeBackend::new()?,display,area})}
    fn require_position(&mut self)->MediaResult<()>{
        let(x,y)=self.position()?;if !contains(self.area,x,y){return Err("Pointer left approved display".into());}Ok(())
    }
    fn require_foreground(&mut self)->MediaResult<()>{
        // No key presses to a foreground window on a different monitor.
        let _dpi=DpiGuard::new()?;
        let(x,y)=self.area.map(32768,32768)?;
        let approved=unsafe{MonitorFromPoint(POINT{x,y},MONITOR_DEFAULTTONULL)};
        let active=unsafe{GetForegroundWindow()};
        if active.is_null()||approved.is_null()||unsafe{MonitorFromWindow(active,MONITOR_DEFAULTTONULL)}!=approved{
            return Err("Foreground window is not on the approved display".into());
        }
        Ok(())
    }
}
impl Backend for ScopedBackend{
    fn check_display(&mut self,area:DisplayArea)->InputResult<()>{
        if area!=self.area{return Err("Approved display changed".into());}
        monitor(self.display,Some(area),true,true)?;Ok(())
    }
    fn move_to(&mut self,x:i32,y:i32)->InputResult<()>{
        if !contains(self.area,x,y){return Err("Pointer outside approved display".into());}
        let _dpi=DpiGuard::new()?;
        // SAFETY: ordinary physical cursor positioning, bounded to selected rectangle.
        if unsafe{SetCursorPos(x,y)}==0{return Err("Windows cursor positioning failed".into());}
        let actual=self.position()?;
        if actual!=(x,y){return Err("Actual cursor differs from authorized point".into());}Ok(())
    }
    fn position(&mut self)->InputResult<(i32,i32)>{
        let _dpi=DpiGuard::new()?;let mut p=POINT{x:0,y:0};
        if unsafe{GetCursorPos(&mut p)}==0{return Err("Cannot verify cursor position".into());}
        Ok((p.x,p.y))
    }
    fn key(&mut self,usage:u16,down:bool)->InputResult<()>{
        if down{self.require_position()?;self.require_foreground()?;}
        // Releases are never dependent on focus or current monitor.
        self.inner.key(usage,down)
    }
    fn button(&mut self,id:u8,down:bool)->InputResult<()>{if down{self.require_position()?;}self.inner.button(id,down)}
    fn scroll_steps(&mut self,x:i32,y:i32)->InputResult<()>{self.require_position()?;self.inner.scroll_steps(x,y)}
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn negative_origin_bounds(){
        let area=DisplayArea{x:-1920,y:-240,width:1920,height:1080};
        assert!(contains(area,-1920,-240));assert!(contains(area,-1,839));assert!(!contains(area,0,839));
        assert_eq!(area.map(0,0).unwrap(),(-1920,-240));assert_eq!(area.map(65535,65535).unwrap(),(-1,839));
    }
}
