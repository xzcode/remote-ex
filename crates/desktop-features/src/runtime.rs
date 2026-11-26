//! Bounded, non-identifying local runtime report exchanged with an installed
//! media component. This is not an OS permission or connectivity test.
use serde::{Serialize,Deserialize};
pub const CORE:&[&str]=&["webrtcbin","nicesrc","nicesink","dtlssrtpenc","dtlssrtpdec","sctpenc","sctpdec","rtpbin","rtpvp8pay","rtpvp8depay","vp8enc","vp8dec","appsrc","appsink","queue","videoconvert","videoscale","videorate","capsfilter","videotestsrc"];
pub const AUDIO:&[&str]=&["opusenc","opusdec","rtpopuspay","rtpopusdepay","audioconvert","audioresample","autoaudiosink","volume","valve"];
pub const OPTIONAL:&[&str]=&["autovideosink","pipewiresrc","wasapi2src"];
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(deny_unknown_fields)]
pub struct Probe{pub schema:u32,pub initialized:bool,pub version:Option<[u32;4]>,pub missing:Vec<String>,pub synthetic_video:bool}
impl Probe{
    pub fn validate(&self)->Result<(),String>{
        if self.schema!=1||self.initialized!=self.version.is_some()||!self.initialized&&self.synthetic_video{return Err("Invalid media probe status".into());}
        let allowed=CORE.iter().chain(AUDIO).chain(OPTIONAL);
        let mut set=std::collections::BTreeSet::new();
        if self.missing.len()>CORE.len()+AUDIO.len()+OPTIONAL.len(){return Err("Media report exceeds bounds".into());}
        for n in &self.missing{if !allowed.clone().any(|k|*k==n.as_str())||!set.insert(n){return Err("Unexpected media element in report".into());}}
        if self.version.is_some_and(|v|v.iter().any(|n|*n>65535)){return Err("Invalid runtime version".into());}
        Ok(())
    }
    pub fn core_ready(&self)->bool{self.initialized&&self.version.is_some_and(|v|v[0]==1&&v[1]>=22)&&self.synthetic_video&&CORE.iter().all(|n|!self.missing.iter().any(|m|m.as_str()==*n))}
    pub fn audio_ready(&self)->bool{self.core_ready()&&AUDIO.iter().all(|n|!self.missing.iter().any(|m|m.as_str()==*n))}
    pub fn portal_ready(&self)->bool{self.core_ready()&&!self.missing.iter().any(|m|m=="pipewiresrc")}
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn report_separates_capabilities(){
        let mut p=Probe{schema:1,initialized:true,version:Some([1,26,11,0]),missing:vec!["pipewiresrc".into()],synthetic_video:true};
        assert!(p.validate().is_ok()&&p.core_ready()&&p.audio_ready()&&!p.portal_ready());
        p.missing.push("opusdec".into());assert!(p.core_ready()&&!p.audio_ready());
        p.missing.push("webrtcbin".into());assert!(!p.core_ready());
        p.missing.push("/home/user".into());assert!(p.validate().is_err());
    }
}
