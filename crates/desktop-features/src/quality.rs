//! Negotiated maximum canvas and bounded runtime encoder quality tiers.
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

/// Auto=0, Full=1, Balanced=2, Low=3. Tiers 1..=3 are actual settings.
pub fn runtime_tier(max:Quality,tier:u8)->Result<Quality,String>{
    if !matches!(tier,1..=3)||max.width<16||max.height<16||max.width>3840||max.height>2160||max.fps==0||max.fps>60||!(100_000..=20_000_000).contains(&max.bitrate){return Err("Invalid quality ceiling or tier".into());}
    let scale=|n:u32|match tier{1=>n,2=>(n*3/4)&!1,_=>(n/2)&!1};
    Ok(Quality{width:scale(max.width).max(16),height:scale(max.height).max(16),
        fps:match tier{1=>max.fps,2=>max.fps.min(20),_=>max.fps.min(10)},
        bitrate:match tier{1=>max.bitrate,2=>(max.bitrate/2).max(100_000),_=>(max.bitrate/4).max(100_000)}})
}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub struct AdaptiveQuality{mode:u8,tier:u8,bad:u8,good:u8}
impl Default for AdaptiveQuality{fn default()->Self{Self{mode:0,tier:1,bad:0,good:0}}}
impl AdaptiveQuality{
    pub fn mode(self)->u8{self.mode}
    pub fn tier(self)->u8{self.tier}
    pub fn select(&mut self,mode:u8)->Result<(),String>{
        if mode>3{return Err("Unknown quality mode".into());}
        self.mode=mode;self.bad=0;self.good=0;if mode!=0{self.tier=mode;}Ok(())
    }
    /// Unknown stats never count as good samples.
    pub fn observe(&mut self,loss:Option<f64>,rtt:Option<u32>)->bool{
        if self.mode!=0{return false;}
        let loss=loss.filter(|v|v.is_finite()&&(0.0..=100.0).contains(v));
        let rtt=rtt.filter(|v|*v<=120_000);
        let bad=loss.is_some_and(|v|v>5.0)||rtt.is_some_and(|v|v>300);
        let good=loss.is_some_and(|v|v<1.0)&&rtt.is_some_and(|v|v<120);
        if bad{self.bad=self.bad.saturating_add(1);self.good=0;}
        else if good{self.good=self.good.saturating_add(1);self.bad=0;}
        else{self.good=0;self.bad=0;}
        let before=self.tier;
        if self.bad>=3{self.tier=(self.tier+1).min(3);self.bad=0;}
        if self.good>=8{self.tier=self.tier.saturating_sub(1).max(1);self.good=0;}
        before!=self.tier
    }
}
#[cfg(test)]
mod runtime_tests{
    use super::*;
    #[test]fn tier_bounds_and_adaptive_hysteresis(){
        for p in ["compat","low","balanced","high","qhd","uhd"]{
            let q=preset(p).unwrap();
            for t in 1..=3{let c=runtime_tier(q,t).unwrap();assert!(c.width<=q.width&&c.height<=q.height&&c.width%2==0&&c.height%2==0&&c.fps<=q.fps&&c.bitrate<=q.bitrate);}
        }
        let mut a=AdaptiveQuality::default();
        assert!(!a.observe(Some(8.0),Some(350)));assert!(!a.observe(Some(8.0),Some(350)));
        assert!(a.observe(Some(8.0),Some(350)));assert_eq!(a.tier(),2);
        for _ in 0..20{assert!(!a.observe(None,None));}
        for _ in 0..7{assert!(!a.observe(Some(0.0),Some(50)));}
        assert!(a.observe(Some(0.0),Some(50)));assert_eq!(a.tier(),1);
        a.select(3).unwrap();for _ in 0..20{assert!(!a.observe(Some(0.0),Some(50)));}
        assert_eq!(a.tier(),3);assert!(a.select(4).is_err());
    }
}
#[cfg(test)]mod tests{use super::*;#[test]fn bounded_profiles(){for p in ["compat","low","balanced","high","qhd","uhd"]{let q=preset(p).unwrap();assert!(q.width*q.height<=3840*2160&&q.fps<=30&&q.bitrate<=20_000_000);}assert!(preset("arbitrary").is_err());}}
