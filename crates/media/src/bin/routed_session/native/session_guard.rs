//! Fail-closed desktop/suspend checks. No unlock, elevation or switching
//! desktops. macOS public APIs report console/login/sleep, not a guaranteed
//! lock-screen signal; physical lock testing remains mandatory.
use super::*;
pub(super) fn interactive()->MediaResult<()>{
    #[cfg(target_os="windows")]
    {
        use std::ffi::c_void;
        type Handle=*mut c_void;
        #[link(name="user32")]
        extern "system"{
            fn OpenInputDesktop(flags:u32,inherit:i32,access:u32)->Handle;
            fn CloseDesktop(desktop:Handle)->i32;
            fn GetUserObjectInformationW(object:Handle,index:i32,info:*mut c_void,length:u32,needed:*mut u32)->i32;
        }
        #[link(name="kernel32")]
        extern "system"{
            fn GetCurrentProcessId()->u32;
            fn ProcessIdToSessionId(process:u32,session:*mut u32)->i32;
            fn WTSGetActiveConsoleSessionId()->u32;
        }
        // SAFETY: all out-pointers point to bounded stack storage; owned
        // desktop handle is closed on every path. Only READOBJECTS access.
        let mut current=0u32;
        let active=unsafe{WTSGetActiveConsoleSessionId()};
        if active==u32::MAX||unsafe{ProcessIdToSessionId(GetCurrentProcessId(),&mut current)}==0||current!=active{return Err("No active local Windows console session".into());}
        let desktop=unsafe{OpenInputDesktop(0,0,1)};
        if desktop.is_null(){return Err("Windows input desktop unavailable (locked or secure desktop)".into());}
        let mut name=[0u16;128];let mut needed=0u32;
        let ok=unsafe{GetUserObjectInformationW(desktop,2,name.as_mut_ptr().cast(),256,&mut needed)};
        unsafe{CloseDesktop(desktop);}
        let end=name.iter().position(|c|*c==0).unwrap_or(name.len());
        if ok==0||needed>256||!String::from_utf16_lossy(&name[..end]).eq_ignore_ascii_case("Default"){return Err("Windows secure or non-default desktop; stop remote sharing".into());}
    }
    #[cfg(target_os="macos")]
    {
        use std::ffi::{c_char,c_void};type Ref=*const c_void;
        #[link(name="CoreGraphics",kind="framework")]
        extern "C"{fn CGSessionCopyCurrentDictionary()->Ref;fn CGMainDisplayID()->u32;fn CGDisplayIsAsleep(display:u32)->u32;}
        #[link(name="CoreFoundation",kind="framework")]
        extern "C"{fn CFRelease(value:Ref);fn CFStringCreateWithCString(alloc:Ref,text:*const c_char,encoding:u32)->Ref;fn CFDictionaryGetValue(dict:Ref,key:Ref)->Ref;fn CFGetTypeID(value:Ref)->usize;fn CFBooleanGetTypeID()->usize;fn CFBooleanGetValue(value:Ref)->u8;}
        // SAFETY: documented Copy/Create objects are retained until release;
        // borrowed dictionary values are type-checked before reading.
        let dict=unsafe{CGSessionCopyCurrentDictionary()};
        if dict.is_null(){return Err("No Quartz GUI session".into());}
        let check=|name:&[u8]|{
            let key=unsafe{CFStringCreateWithCString(std::ptr::null(),name.as_ptr().cast(),0x08000100)};
            if key.is_null(){return false;}
            let value=unsafe{CFDictionaryGetValue(dict,key)};unsafe{CFRelease(key);}
            !value.is_null()&&unsafe{CFGetTypeID(value)==CFBooleanGetTypeID()&&CFBooleanGetValue(value)!=0}
        };
        let ok=check(b"kCGSessionOnConsoleKey\0")&&check(b"kCGSessionLoginDoneKey\0");
        unsafe{CFRelease(dict);}
        if !ok||unsafe{CGDisplayIsAsleep(CGMainDisplayID())}!=0{return Err("macOS console unavailable or display asleep".into());}
    }
    Ok(())
}
pub(in crate::routed_session) fn start()->MediaResult<()>{
    interactive()?;
    std::thread::spawn(||{
        let mut clock=remote_ex_desktop_features::platform::LifecycleWatch::default();
        while !canceled(){
            std::thread::sleep(Duration::from_millis(500));
            if let Err(e)=clock.check().and_then(|_|interactive()){
                CANCEL.store(true,Ordering::Release);eprintln!("FAIL: {e}");break;
            }
        }
    });Ok(())
}
