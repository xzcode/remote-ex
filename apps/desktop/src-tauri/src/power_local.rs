//! Local-only final execution after verified review and clean route closure.
//! No shell, arbitrary commands, forced shutdown, elevation or remote flags.
use remote_ex_desktop_features::power::PowerAction;
use std::{path::PathBuf,process::{Command,Stdio},sync::atomic::{AtomicBool,Ordering},time::{Duration,Instant}};
fn command(action:PowerAction)->Result<Command,String>{
    #[cfg(target_os="windows")]
    {
        let root=PathBuf::from(std::env::var_os("SystemRoot").ok_or("Windows system directory unavailable")?);
        if !root.is_absolute(){return Err("Invalid local Windows directory".into());}
        let path=root.join("System32").join("shutdown.exe");
        if !path.is_file(){return Err("Windows power utility unavailable".into());}
        let mut c=Command::new(path);
        // /t >0 can imply force on Windows. Zero is intentional; never /f.
        c.args([if action==PowerAction::Shutdown{"/s"}else{"/r"},"/t","0"]);return Ok(c);
    }
    #[cfg(target_os="macos")]
    {
        let mut c=Command::new("/usr/bin/osascript");
        c.args(["-e",if action==PowerAction::Shutdown{"tell application \"System Events\" to shut down"}else{"tell application \"System Events\" to restart"}]);return Ok(c);
    }
    #[cfg(target_os="linux")]
    {
        let mut c=Command::new("/usr/bin/systemctl");
        c.arg(if action==PowerAction::Shutdown{"poweroff"}else{"reboot"});return Ok(c);
    }
    #[allow(unreachable_code)]
    {let _=action;Err("OS power command not supported".into())}
}
pub(crate) fn submit(action:PowerAction,stop:&AtomicBool)->Result<(),String>{
    if stop.load(Ordering::Acquire){return Err("Local power approval revoked".into());}
    let mut cmd=command(action)?;
    if stop.load(Ordering::Acquire){return Err("Local power approval revoked".into());}
    let mut child=cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map_err(|_|"Cannot submit local OS power request")?;
    let deadline=Instant::now()+Duration::from_secs(12);
    loop{
        match child.try_wait(){
            Ok(Some(status))=>return if status.success(){Ok(())}else{Err("OS refused the local power request".into())},
            Ok(None) if !stop.load(Ordering::Acquire)&&Instant::now()<deadline=>std::thread::sleep(Duration::from_millis(50)),
            _=>{let _=child.kill();let _=child.wait();return Err("Power utility interrupted; OS outcome unknown".into());}
        }
    }
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn closed_command_vocabulary_never_requests_force(){
        for a in [PowerAction::Shutdown,PowerAction::Restart]{
            if let Ok(c)=command(a){
                assert!(std::path::Path::new(c.get_program()).is_absolute());
                assert!(c.get_args().all(|v|v!="/f"&&v!="--force"&&v!="-f"&&v!="sudo"));
            }
        }
        // Never spawn a real power command in any test.
        assert!(submit(PowerAction::Shutdown,&AtomicBool::new(true)).is_err());
    }
}
