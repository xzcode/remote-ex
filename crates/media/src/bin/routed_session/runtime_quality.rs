//! Versioned control records only in separately signed extended profiles.
use super::*;
use remote_ex_desktop_features::quality::{AdaptiveQuality,Quality,runtime_tier};
pub(super) const REQUEST:u8=0xD2;
pub(super) const STATUS:u8=0xD3;
pub(super) const NATIVE_SECONDS:u64=300;
fn tier(config:VideoConfig,n:u8)->MediaResult<VideoConfig>{
    let q=runtime_tier(Quality{width:config.width,height:config.height,fps:config.fps,bitrate:config.bitrate},n)?;
    VideoConfig{width:q.width,height:q.height,fps:q.fps,bitrate:q.bitrate}.validate()
}
#[derive(Default)]
pub(super) struct HostQuality{state:AdaptiveQuality,last_request:Option<Instant>}
impl HostQuality{
    pub(super) fn publish(&self,peer:&Peer,channel:&ControlChannel)->MediaResult<()>{
        let c=tier(peer.config(),self.state.tier())?;
        peer.set_stream_quality(c)?;
        channel.send(&[STATUS,self.state.mode(),self.state.tier()])?;
        log_status(peer.config(),self.state.mode(),self.state.tier())
    }
    pub(super) fn request(&mut self,bytes:&[u8],peer:&Peer,channel:&ControlChannel)->MediaResult<()>{
        if bytes.len()!=2||bytes[0]!=REQUEST||bytes[1]>3{return Err("Invalid stream quality request".into());}
        if self.last_request.is_some_and(|at|at.elapsed()<Duration::from_millis(750)){return Err("Stream quality requests too frequent".into());}
        self.last_request=Some(Instant::now());self.state.select(bytes[1])?;self.publish(peer,channel)
    }
    pub(super) fn observe(&mut self,peer:&Peer,channel:&ControlChannel,loss:Option<f64>,rtt:Option<u32>)->MediaResult<()>{
        if self.state.observe(loss,rtt){self.publish(peer,channel)?;}Ok(())
    }
}
pub(super) fn status(bytes:&[u8])->MediaResult<(u8,u8)>{
    if bytes.len()!=3||bytes[0]!=STATUS||bytes[1]>3||!(1..=3).contains(&bytes[2])||bytes[1]!=0&&bytes[1]!=bytes[2]{return Err("Invalid stream quality status".into());}
    Ok((bytes[1],bytes[2]))
}
pub(super) fn log_status(config:VideoConfig,mode:u8,level:u8)->MediaResult<()>{
    let c=tier(config,level)?;
    out(&format!("STREAM_QUALITY {mode} {level} {} {} {} {}",c.width,c.height,c.fps,c.bitrate))
}
/// Actual decoder test, inert input only. Both switches then >60s session.
pub(super) async fn test_controller(peer:&Peer,channel:&ControlChannel,conn:&mut Connection,session:&str,config:VideoConfig)->MediaResult<()>{
    let start=Instant::now();let mut phase=0u8;let mut requested_at=start;
    let mut last_heartbeat=start-Duration::from_secs(1);
    let mut last_frame=start;let mut frames=0u32;
    let mut actual=None;let mut ack=None;
    while start.elapsed()<Duration::from_secs(72){
        if canceled(){return Err("Extended test revoked".into());}
        connected(peer,channel)?;
        if last_heartbeat.elapsed()>=Duration::from_millis(300){channel.send(HEARTBEAT)?;last_heartbeat=Instant::now();}
        while let Some(b)=channel.receive()?{let s=status(&b)?;ack=Some(s);log_status(config,s.0,s.1)?;}
        if let Some(frame)=peer.pull_frame(Duration::from_millis(25))?{
            let info=Peer::frame_info(&frame)?;
            if info.width!=config.width||info.height!=config.height{return Err("Viewer canvas changed during quality switch".into());}
            actual=peer.decoded_stream_dimensions()?;frames=frames.saturating_add(1);last_frame=Instant::now();
        }
        if last_frame.elapsed()>Duration::from_secs(5){return Err("Extended video stalled".into());}
        let full=(config.width,config.height);let low=tier(config,3)?;
        if phase==0&&ack==Some((0,1))&&actual==Some(full)&&frames>=10{
            channel.send(&[REQUEST,3])?;requested_at=Instant::now();phase=1;
        }else if phase==1&&ack==Some((3,3))&&actual==Some((low.width,low.height))&&requested_at.elapsed()>=Duration::from_secs(2){
            out(&format!("ADAPTIVE_LOW {} {}",low.width,low.height))?;
            channel.send(&[REQUEST,1])?;requested_at=Instant::now();phase=2;
        }else if phase==2&&ack==Some((1,1))&&actual==Some(full){
            out(&format!("ADAPTIVE_FULL {} {}",config.width,config.height))?;phase=3;
        }
        if phase<3&&requested_at.elapsed()>Duration::from_secs(12){return Err("Actual decoder quality switch timed out".into());}
        tick(conn).await?;
    }
    if phase!=3||frames<100{return Err("Extended media validation incomplete".into());}
    channel.send(STOP)?;wait_data(peer,channel,conn,ACK).await?;
    conn.send(Client::Close{session:session.into()}).await?;finish(conn,session).await?;
    out(&format!("PASS ROUTED CONTROLLER: extended_seconds={}, decoded_frames={frames}, release_ack=true",start.elapsed().as_secs()))
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn strict_status_records(){assert_eq!(status(&[STATUS,0,2]).unwrap(),(0,2));assert!(status(&[STATUS,3,1]).is_err());assert!(status(&[STATUS,4,2]).is_err());assert!(status(&[STATUS,1]).is_err());}
}
