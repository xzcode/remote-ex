//! A closed power-action vocabulary and session-bound local review.
use serde::{Serialize,Deserialize};
use sha2::{Digest,Sha256};
#[derive(Clone,Copy,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(rename_all="snake_case")]
pub enum PowerAction{Shutdown,Restart}
impl PowerAction{
    pub fn parse(s:&str)->Result<Self,String>{match s{"shutdown"=>Ok(Self::Shutdown),"restart"=>Ok(Self::Restart),_=>Err("Unsupported power action".into())}}
    pub fn name(self)->&'static str{match self{Self::Shutdown=>"shutdown",Self::Restart=>"restart"}}
    pub fn code(self)->u8{match self{Self::Shutdown=>1,Self::Restart=>2}}
    pub fn from_code(n:u8)->Result<Self,String>{match n{1=>Ok(Self::Shutdown),2=>Ok(Self::Restart),_=>Err("Invalid power action code".into())}}
    pub fn review(self,session:[u8;16])->Result<PowerReview,String>{
        if session==[0;16]{return Err("Invalid power review session".into());}
        let mut h=Sha256::new();h.update(b"Remote EX/local-power-review/v1\0");h.update(session);h.update([self.code()]);
        Ok(PowerReview{action:self,token:remote_ex_signaling::hex(&h.finalize())})
    }
}
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PowerReview{pub action:PowerAction,pub token:String}
impl PowerReview{
    pub fn validate(&self)->Result<(),String>{
        if self.token.len()!=64||!self.token.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b)){return Err("Invalid power review token".into());}Ok(())
    }
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn exact_action_and_session_are_bound(){
        let a=PowerAction::Shutdown.review([1;16]).unwrap();a.validate().unwrap();
        assert_ne!(a,PowerAction::Shutdown.review([2;16]).unwrap());
        assert_ne!(a.token,PowerAction::Restart.review([1;16]).unwrap().token);
        assert!(PowerAction::Restart.review([0;16]).is_err());
        for x in ["sleep","shutdown /f","reboot; bash",""]{assert!(PowerAction::parse(x).is_err());}
        assert!(PowerAction::from_code(0).is_err());
    }
}
