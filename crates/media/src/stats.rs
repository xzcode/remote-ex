//! Only numeric counters and actual selected candidate classification.
//! Never print raw GStreamer stats, candidate addresses or credentials.
use crate::Peer;
use gstreamer as gst;
use std::{sync::mpsc::{Receiver,TryRecvError},time::{Duration,Instant}};
#[derive(Clone,Default,Debug)]
pub struct Counters{
    pub bytes:Option<u64>,pub received:Option<u64>,pub lost:Option<u64>,
    pub rtt:Option<f64>,pub fraction_lost:Option<f64>,pub route:Option<&'static str>,
}
pub fn parse(report:&gst::StructureRef)->Counters{
    let mut c=Counters::default();
    for (_,value) in report.iter(){
        let Ok(s)=value.get::<gst::Structure>()else{continue};
        for field in ["bytes-sent","bytes-received"]{
            if let Ok(v)=s.get::<u64>(field){c.bytes=Some(c.bytes.unwrap_or(0).saturating_add(v));}
        }
        if let Ok(v)=s.get::<u64>("packets-received"){c.received=Some(c.received.unwrap_or(0).saturating_add(v));}
        if let Ok(v)=s.get::<i64>("packets-lost"){if v>=0{c.lost=Some(c.lost.unwrap_or(0).saturating_add(v as u64));}}
        if let Ok(rtt)=s.get::<f64>("round-trip-time"){if rtt.is_finite()&&(0.0..=120.0).contains(&rtt){c.rtt=Some(c.rtt.unwrap_or(0.0).max(rtt));}}
        if let Ok(loss)=s.get::<f64>("fraction-lost"){if loss.is_finite()&&(0.0..=1.0).contains(&loss){c.fraction_lost=Some(c.fraction_lost.unwrap_or(0.0).max(loss));}}
        if let Ok(pair_id)=s.get::<String>("selected-candidate-pair-id"){
            if let Ok(pair)=report.get::<gst::Structure>(pair_id.as_str()){
                let candidate=|field:&str|pair.get::<String>(field).ok().and_then(|id|report.get::<gst::Structure>(id.as_str()).ok()).and_then(|s|s.get::<String>("candidate-type").ok());
                if let(Some(local),Some(remote))=(candidate("local-candidate-id"),candidate("remote-candidate-id")){
                    let r=if local=="relay"||remote=="relay"{"relay"}else if ["host","srflx","prflx"].contains(&local.as_str())&&["host","srflx","prflx"].contains(&remote.as_str()){"direct"}else{"unknown"};
                    c.route=Some(if c.route==Some("relay"){"relay"}else{r});
                }
            }
        }
    }
    c
}
#[derive(Debug,Default)]
pub struct Sample{pub bitrate:Option<u64>,pub rtt_ms:Option<u32>,pub loss_pct:Option<f64>,pub route:Option<&'static str>}
impl Sample{
    pub fn line(&self)->String{
        let b=self.bitrate.map(|n|n.to_string()).unwrap_or_else(||"-".into());
        let r=self.rtt_ms.map(|n|n.to_string()).unwrap_or_else(||"-".into());
        let l=self.loss_pct.map(|n|format!("{n:.2}")).unwrap_or_else(||"-".into());
        format!("NET_STATS {b} {r} {l} {}",self.route.unwrap_or("unknown"))
    }
}
pub struct Sampler{pending:Option<(Instant,Receiver<Option<Counters>>)>,previous:Option<(Instant,Counters)>,next:Instant,disabled:bool}
impl Default for Sampler{fn default()->Self{Self{pending:None,previous:None,next:Instant::now(),disabled:false}}}
impl Sampler{
    pub fn poll(&mut self,peer:&Peer)->Option<Sample>{
        if self.disabled{return None;}
        let now=Instant::now();
        if let Some((started,rx))=&self.pending{
            match rx.try_recv(){
                Ok(Some(c))=>{
                    self.pending=None;self.next=now+Duration::from_secs(1);
                    let mut sample=Sample{rtt_ms:c.rtt.map(|x|(x*1000.0) as u32),route:c.route,..Default::default()};
                    if let Some((at,old))=&self.previous{
                        let seconds=now.saturating_duration_since(*at).as_secs_f64();
                        if seconds>0.0{
                            sample.bitrate=c.bytes.zip(old.bytes).and_then(|(a,b)|a.checked_sub(b)).map(|d|(d as f64*8.0/seconds)as u64);
                        }
                        let rx=c.received.zip(old.received).and_then(|(a,b)|a.checked_sub(b));
                        let loss=c.lost.zip(old.lost).and_then(|(a,b)|a.checked_sub(b));
                        if let(Some(rx),Some(loss))=(rx,loss){let total=rx.saturating_add(loss);if total>0{sample.loss_pct=Some(loss as f64*100.0/total as f64);}}
                    }
                    if sample.loss_pct.is_none(){sample.loss_pct=c.fraction_lost.map(|x|x*100.0);}
                    self.previous=Some((now,c));return Some(sample);
                }
                Ok(None)|Err(TryRecvError::Disconnected)=>{self.pending=None;self.next=now+Duration::from_secs(1);}
                Err(TryRecvError::Empty) if now.saturating_duration_since(*started)>Duration::from_secs(3)=>{self.pending=None;self.disabled=true;}
                Err(TryRecvError::Empty)=>{},
            }
        }else if now>=self.next{self.pending=Some((now,peer.request_transport_stats()));}
        None
    }
}
