//! Fixed quality presets. A resolution change requires new negotiation.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct Quality{pub width:u32,pub height:u32,pub fps:u32,pub bitrate:u32}
pub fn preset(name:&str)->Result<Quality,String>{
    let (width,height,fps,bitrate)=match name{
        "compat"=>(960,540,10,1_500_000),
        "low"=>(854,480,15,800_000),
        "balanced"=>(1280,720,30,2_500_000),
        "high"=>(1920,1080,30,5_000_000),
        "qhd"=>(2560,1440,30,8_000_000),
        "uhd"=>(3840,2160,30,16_000_000),
        _=>return Err("Unknown quality preset".into()),
    };
    Ok(Quality{width,height,fps,bitrate})
}
pub fn code(name:&str)->Result<u8,String>{match name{"compat"=>Ok(0),"low"=>Ok(1),"balanced"=>Ok(2),"high"=>Ok(3),"qhd"=>Ok(4),"uhd"=>Ok(5),_=>Err("Unknown quality preset".into())}}
#[cfg(test)]mod tests{use super::*;#[test]fn bounded_profiles(){for p in ["compat","low","balanced","high","qhd","uhd"]{let q=preset(p).unwrap();assert!(q.width*q.height<=3840*2160&&q.fps<=30&&q.bitrate<=20_000_000);}assert!(preset("arbitrary").is_err());}}
