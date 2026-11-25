//! Native viewer, focus-gated input; no global input hooks.
use super::{canceled,connected,Geometry,CONFIG,HEARTBEAT};
use minifb::{Key as K,MouseButton,MouseMode,Window,WindowOptions};
use prost::Message as _;
use remote_ex_media::{MediaResult,Peer,ControlChannel};
use remote_ex_protocol::{InputEnvelope,VERSION,input_envelope::Event,Key,PointerButton,PointerMove,Scroll,ReleaseAll};
use std::{collections::BTreeSet,time::{Duration,Instant}};
fn hid(k:K)->Option<u16>{
    let n=k as u16;
    if (K::A as u16..=K::Z as u16).contains(&n){return Some(n-K::A as u16+4);}
    if (K::Key1 as u16..=K::Key9 as u16).contains(&n){return Some(n-K::Key1 as u16+30);}
    if (K::F1 as u16..=K::F12 as u16).contains(&n){return Some(n-K::F1 as u16+58);}
    Some(match k{
        K::Key0=>39,K::Enter=>40,K::Backspace=>42,K::Tab=>43,K::Space=>44,
        K::Minus=>45,K::Equal=>46,K::LeftBracket=>47,K::RightBracket=>48,K::Backslash=>49,
        K::Semicolon=>51,K::Apostrophe=>52,K::Backquote=>53,K::Comma=>54,K::Period=>55,K::Slash=>56,K::CapsLock=>57,
        K::Home=>74,K::PageUp=>75,K::Delete=>76,K::End=>77,K::PageDown=>78,K::Right=>79,K::Left=>80,K::Down=>81,K::Up=>82,
        K::LeftCtrl=>224,K::LeftShift=>225,K::LeftAlt=>226,K::LeftSuper=>227,K::RightCtrl=>228,K::RightShift=>229,K::RightAlt=>230,K::RightSuper=>231,
        _=>return None,
    })
}
fn pointer(g:Geometry,x:f32,y:f32)->Option<(u32,u32)>{
    if !x.is_finite()||!y.is_finite()||x<g.x as f32||y<g.y as f32||x>=(g.x+g.width)as f32||y>=(g.y+g.height)as f32{return None;}
    let norm=|v:f32,o:u32,n:u32|(((v-o as f32)/(n-1)as f32).clamp(0.,1.)*65535.).round()as u32;
    Some((norm(x,g.x,g.width),norm(y,g.y,g.height)))
}
pub fn run(peer:&Peer,channel:&ControlChannel,session:[u8;16],geometry:Geometry)->MediaResult<u32>{
    let(w,h)=(CONFIG.width as usize,CONFIG.height as usize);
    let mut window=Window::new("Remote EX — authorized control (Esc disconnects)",w,h,WindowOptions{resize:false,..Default::default()}).map_err(|e|e.to_string())?;
    window.set_target_fps(60);
    let mut seq=0u64;
    let mut emit=|event:Event|->MediaResult<()>{
        seq=seq.checked_add(1).ok_or("Input sequence exhausted")?;
        channel.send(&InputEnvelope{version:VERSION,session_id:session.to_vec(),sequence:seq,event:Some(event)}.encode_to_vec())
    };
    let mut buffer=vec![0u32;w*h];let mut count=0u32;
    let mut keys=BTreeSet::new();let mut buttons=BTreeSet::new();
    let mut armed=false;let mut previous=None;let mut last_frame=Instant::now();
    let mut heartbeat=Instant::now()-Duration::from_secs(1);let started=Instant::now();
    let result=(||{
        while window.is_open()&&!canceled()&&started.elapsed()<Duration::from_secs(28){
            connected(peer,channel)?;
            if heartbeat.elapsed()>=Duration::from_millis(300){channel.send(HEARTBEAT)?;heartbeat=Instant::now();}
            if let Some(sample)=peer.pull_frame(Duration::ZERO)?{
                let info=Peer::frame_info(&sample)?;
                let b=sample.buffer().ok_or("Missing native frame")?;
                let map=b.map_readable().map_err(|_|"Cannot map decoded frame")?;
                let data=map.as_slice();
                if info.width!=CONFIG.width||info.height!=CONFIG.height||data.len()!=w*h*4{return Err("Unsupported decoded frame stride".into());}
                for(dst,p)in buffer.iter_mut().zip(data.chunks_exact(4)){*dst=(u32::from(p[0])<<16)|(u32::from(p[1])<<8)|u32::from(p[2]);}
                count+=1;last_frame=Instant::now();
            }
            window.update_with_buffer(&buffer,w,h).map_err(|e|e.to_string())?;
            if window.is_key_down(K::Escape){break;}
            if last_frame.elapsed()>Duration::from_secs(2){return Err("Video stalled; input stopped".into());}
            let pos=window.get_mouse_pos(MouseMode::Discard).and_then(|(x,y)|pointer(geometry,x,y));
            let current_keys:BTreeSet<u16>=window.get_keys().into_iter().filter_map(hid).collect();
            let current_buttons:BTreeSet<u8>=[(1,MouseButton::Left),(2,MouseButton::Middle),(3,MouseButton::Right)].into_iter().filter_map(|(n,b)|window.get_mouse_down(b).then_some(n)).collect();
            if count==0||!window.is_active()||pos.is_none(){
                if armed{emit(Event::ReleaseAll(ReleaseAll{}))?;}
                armed=false;keys.clear();buttons.clear();previous=None;continue;
            }
            if !armed{
                if current_keys.is_empty()&&current_buttons.is_empty(){armed=true;}else{continue;}
            }
            let(x,y)=pos.unwrap();
            if previous!=Some((x,y)){emit(Event::PointerMove(PointerMove{x,y}))?;previous=Some((x,y));}
            for &usage in keys.difference(&current_keys){emit(Event::Key(Key{usage:u32::from(usage),down:false}))?;}
            for &usage in current_keys.difference(&keys){emit(Event::Key(Key{usage:u32::from(usage),down:true}))?;}
            for &button in buttons.difference(&current_buttons){emit(Event::PointerButton(PointerButton{button:u32::from(button),down:false}))?;}
            for &button in current_buttons.difference(&buttons){emit(Event::PointerButton(PointerButton{button:u32::from(button),down:true}))?;}
            keys=current_keys;buttons=current_buttons;
            if let Some((x,y))=window.get_scroll_wheel(){
                let units=|v:f32|if v.is_finite(){(v*120.).round().clamp(-1200.,1200.)as i32}else{0};
                let(x,y)=(units(x),units(-y));
                if x!=0||y!=0{emit(Event::Scroll(Scroll{x,y}))?;}
            }
        }Ok(count)
    })();
    let release=emit(Event::ReleaseAll(ReleaseAll{}));release?;result
}
