//! Routed viewer with a local-only toolbar and borderless fullscreen.
//! Remote events are accepted only in the focused picture, never on the toolbar.
use super::*;
use font8x8::{BASIC_FONTS,UnicodeFonts};
use prost::Message as _;
use remote_ex_desktop_features::viewport::{self,Rect};
use remote_ex_protocol::{InputEnvelope,VERSION,input_envelope::Event,Key,PointerButton,PointerMove,Scroll,ReleaseAll};
use softbuffer::Surface;
use std::{collections::BTreeSet,num::NonZeroU32,rc::Rc};
use winit::{application::ApplicationHandler,dpi::{LogicalSize,PhysicalSize},event::{ElementState,MouseButton,MouseScrollDelta,WindowEvent},event_loop::{ActiveEventLoop,ControlFlow,EventLoop},keyboard::{KeyCode as K,PhysicalKey},window::{Fullscreen,Window,WindowId}};

const MAX_WINDOW_PIXELS:u64=32_000_000;
fn hid(k:K)->Option<u16>{
    // Explicit key mapping; no assumption about enum discriminants.
    let letters=[K::KeyA,K::KeyB,K::KeyC,K::KeyD,K::KeyE,K::KeyF,K::KeyG,K::KeyH,K::KeyI,K::KeyJ,K::KeyK,K::KeyL,K::KeyM,K::KeyN,K::KeyO,K::KeyP,K::KeyQ,K::KeyR,K::KeyS,K::KeyT,K::KeyU,K::KeyV,K::KeyW,K::KeyX,K::KeyY,K::KeyZ];
    if let Some(n)=letters.iter().position(|&v|v==k){return Some(n as u16+4);}
    let digits=[K::Digit1,K::Digit2,K::Digit3,K::Digit4,K::Digit5,K::Digit6,K::Digit7,K::Digit8,K::Digit9,K::Digit0];
    if let Some(n)=digits.iter().position(|&v|v==k){return Some(n as u16+30);}
    let functions=[K::F1,K::F2,K::F3,K::F4,K::F5,K::F6,K::F7,K::F8,K::F9,K::F10,K::F11,K::F12];
    if let Some(n)=functions.iter().position(|&v|v==k){return Some(n as u16+58);}
    Some(match k{K::Enter=>40,K::Backspace=>42,K::Tab=>43,K::Space=>44,K::Minus=>45,K::Equal=>46,K::BracketLeft=>47,K::BracketRight=>48,K::Backslash=>49,K::Semicolon=>51,K::Quote=>52,K::Backquote=>53,K::Comma=>54,K::Period=>55,K::Slash=>56,K::CapsLock=>57,K::Home=>74,K::PageUp=>75,K::Delete=>76,K::End=>77,K::PageDown=>78,K::ArrowRight=>79,K::ArrowLeft=>80,K::ArrowDown=>81,K::ArrowUp=>82,K::ControlLeft=>224,K::ShiftLeft=>225,K::AltLeft=>226,K::SuperLeft=>227,K::ControlRight=>228,K::ShiftRight=>229,K::AltRight=>230,K::SuperRight=>231,_=>return None})
}
struct Viewer<'a>{
    peer:&'a Peer,channel:&'a ControlChannel,session:[u8;16],geometry:Rect,
    allow_input:bool,input_enabled:bool,av:bool,volume:u8,screens:u8,view_index:Option<u8>,switch_pending:bool,
    window:Option<Rc<Window>>,surface:Option<Surface<Rc<Window>,Rc<Window>>>,
    size:PhysicalSize<u32>,toolbar:u32,scale:u32,fit:Option<Rect>,last_cursor:Option<(f64,f64)>,
    focused:bool,armed:bool,physical_keys:BTreeSet<u16>,physical_buttons:BTreeSet<u8>,
    sent_keys:BTreeSet<u16>,sent_buttons:BTreeSet<u8>,previous:Option<(u32,u32)>,seq:u64,
    pixels:Vec<u32>,frames:u32,metric_frames:u32,metric_bytes:u64,last_metric:Instant,last_frame:Instant,
    stats:remote_ex_media::stats::Sampler,started:Instant,heartbeat:Instant,
    released:bool,error:Option<String>,next_frame:Instant,fps:f64,
    extended:bool,quality_mode:u8,quality_pending:Option<(u8,Instant)>,actual_stream:Option<(u32,u32)>,
}
impl Viewer<'_>{
    fn emit(&mut self,event:Event)->MediaResult<()>{
        self.seq=self.seq.checked_add(1).ok_or("Input sequence exhausted")?;
        self.channel.send(&InputEnvelope{version:VERSION,session_id:self.session.to_vec(),sequence:self.seq,event:Some(event)}.encode_to_vec())
    }
    fn disarm(&mut self)->MediaResult<()>{
        if self.armed||!self.sent_keys.is_empty()||!self.sent_buttons.is_empty(){
            if self.allow_input&&!self.released{self.emit(Event::ReleaseAll(ReleaseAll{}))?;}
        }
        self.armed=false;self.sent_keys.clear();self.sent_buttons.clear();self.previous=None;Ok(())
    }
    fn layout(&mut self,size:PhysicalSize<u32>)->MediaResult<()>{
        self.disarm()?;self.size=size;
        self.scale=self.window.as_ref().map(|w|w.scale_factor().round().clamp(1.0,3.0)as u32).unwrap_or(1);
        self.toolbar=(32*self.scale).min(size.height);
        let config=self.peer.config();
        self.fit=viewport::fit((config.width,config.height),Rect{x:0,y:self.toolbar,width:size.width,height:size.height.saturating_sub(self.toolbar)});
        self.last_cursor=None;
        if size.width==0||size.height==0{return Ok(());}
        if u64::from(size.width)*u64::from(size.height)>MAX_WINDOW_PIXELS{return Err("Viewer window exceeds safe pixel limit".into());}
        self.surface.as_mut().ok_or("Missing rendering surface")?.resize(NonZeroU32::new(size.width).unwrap(),NonZeroU32::new(size.height).unwrap()).map_err(error)
    }
    fn pointer(&self)->Option<(u32,u32)>{
        let c=self.peer.config();
        viewport::map_pointer(self.last_cursor?,self.fit?,(c.width,c.height),self.geometry)
    }
    fn reconcile(&mut self)->MediaResult<()>{
        let pos=self.pointer();
        if !self.allow_input||!self.input_enabled||!self.focused||self.frames==0||pos.is_none(){return self.disarm();}
        if !self.armed{
            if self.physical_keys.is_empty()&&self.physical_buttons.is_empty(){self.armed=true;}else{return Ok(());}
        }
        let(x,y)=pos.unwrap();
        if self.previous!=Some((x,y)){self.emit(Event::PointerMove(PointerMove{x,y}))?;self.previous=Some((x,y));}
        let up:Vec<_>=self.sent_keys.difference(&self.physical_keys).copied().collect();
        let down:Vec<_>=self.physical_keys.difference(&self.sent_keys).copied().collect();
        for usage in up{self.emit(Event::Key(Key{usage:u32::from(usage),down:false}))?;self.sent_keys.remove(&usage);}
        for usage in down{self.emit(Event::Key(Key{usage:u32::from(usage),down:true}))?;self.sent_keys.insert(usage);}
        let up:Vec<_>=self.sent_buttons.difference(&self.physical_buttons).copied().collect();
        let down:Vec<_>=self.physical_buttons.difference(&self.sent_buttons).copied().collect();
        for button in up{self.emit(Event::PointerButton(PointerButton{button:u32::from(button),down:false}))?;self.sent_buttons.remove(&button);}
        for button in down{self.emit(Event::PointerButton(PointerButton{button:u32::from(button),down:true}))?;self.sent_buttons.insert(button);}
        Ok(())
    }
    fn local_action(&mut self,n:u32,el:&ActiveEventLoop)->MediaResult<()>{
        self.disarm()?;
        match n{
            0=>el.exit(),
            1=>if let Some(w)=&self.window{w.set_fullscreen(if w.fullscreen().is_some(){None}else{Some(Fullscreen::Borderless(w.current_monitor()))});},
            2 if self.allow_input=>self.input_enabled=!self.input_enabled,
            3 if !self.allow_input&&self.screens>1&&!self.switch_pending=>{self.channel.send(SWITCH_VIEW)?;self.switch_pending=true;},
            4 if self.extended&&self.quality_pending.is_none()=>{
                let next=(self.quality_mode+1)%4;
                self.channel.send(&[runtime_quality::REQUEST,next])?;
                self.quality_pending=Some((next,Instant::now()));
            },
            _=>{},
        }
        Ok(())
    }
    fn poll(&mut self,el:&ActiveEventLoop)->MediaResult<()>{
        if canceled()||self.started.elapsed()>=Duration::from_secs(if self.extended{runtime_quality::NATIVE_SECONDS-2}else{28}){el.exit();return Ok(());}
        for _ in 0..4{
            match self.channel.receive()?{
                None=>break,
                Some(b) if b==ACK=>{self.released=true;el.exit();return Ok(());}
                Some(b) if !self.allow_input&&b.len()==3&&b[0]==VIEW_LAYOUT_ACK&&b[2]==self.screens&&b[1]<=self.screens=>{
                    self.view_index=Some(b[1]);self.switch_pending=false;
                },
                Some(b) if self.extended&&b.first()==Some(&runtime_quality::STATUS)=>{
                    let(mode,level)=runtime_quality::status(&b)?;
                    self.quality_mode=mode;
                    if self.quality_pending.is_some_and(|(requested,_)|requested==mode){self.quality_pending=None;}
                    runtime_quality::log_status(self.peer.config(),mode,level)?;
                },
                Some(_)=>return Err("Unexpected viewer control response".into()),
            }
        }
        if self.quality_pending.is_some_and(|(_,at)|at.elapsed()>Duration::from_secs(5)){return Err("Stream quality response timed out".into());}
        connected(self.peer,self.channel)?;
        if self.av{let volume=audio::VOLUME.load(Ordering::Acquire);if volume!=self.volume{self.peer.set_audio_volume(volume)?;self.volume=volume;out(&format!("AUDIO_VOLUME {volume}"))?;}}
        if self.heartbeat.elapsed()>=Duration::from_millis(300){self.channel.send(HEARTBEAT)?;self.heartbeat=Instant::now();}
        if let Some(sample)=self.peer.pull_frame(Duration::ZERO)?{
            let info=Peer::frame_info(&sample)?;let config=self.peer.config();
            let buffer=sample.buffer().ok_or("Missing native frame")?;
            let map=buffer.map_readable().map_err(error)?;
            let data=map.as_slice();
            if info.width!=config.width||info.height!=config.height||data.len()!=self.pixels.len()*4{return Err("Unexpected decoded frame geometry".into());}
            for(dst,p)in self.pixels.iter_mut().zip(data.chunks_exact(4)){*dst=(u32::from(p[0])<<16)|(u32::from(p[1])<<8)|u32::from(p[2]);}
            if self.extended{let actual=self.peer.decoded_stream_dimensions()?;if actual!=self.actual_stream{self.actual_stream=actual;if let Some((w,h))=actual{out(&format!("STREAM_DECODED {w} {h}"))?;}}}
            self.frames=self.frames.saturating_add(1);self.metric_frames+=1;self.metric_bytes+=data.len()as u64;self.last_frame=Instant::now();
        }
        if self.last_frame.elapsed()>Duration::from_secs(2){return Err("Video stalled; input stopped".into());}
        if let Some(sample)=self.stats.poll(self.peer){println!("{}",sample.line());}
        if self.last_metric.elapsed()>=Duration::from_secs(1){
            let elapsed=self.last_metric.elapsed().as_secs_f64();self.fps=self.metric_frames as f64/elapsed;
            println!("VIEW_STATS {:.1} {}",self.fps,(self.metric_bytes as f64*8.0/elapsed)as u64);
            self.metric_frames=0;self.metric_bytes=0;self.last_metric=Instant::now();
        }
        self.reconcile()?;
        if let Some(w)=&self.window{w.request_redraw();}
        Ok(())
    }
    fn paint(&mut self)->MediaResult<()>{
        let(w,h)=(self.size.width,self.size.height);if w==0||h==0{return Ok(());}
        let config=self.peer.config();let scale=self.scale;let cell=112*scale;
        let mut buffer=self.surface.as_mut().ok_or("Missing rendering surface")?.buffer_mut().map_err(error)?;
        buffer.fill(0x101418);
        if let Some(f)=self.fit{
            for y in f.y..f.y+f.height{
                let sy=u64::from(y-f.y)*u64::from(config.height)/u64::from(f.height);
                for x in f.x..f.x+f.width{
                    let sx=u64::from(x-f.x)*u64::from(config.width)/u64::from(f.width);
                    buffer[(y*w+x)as usize]=self.pixels[(sy*u64::from(config.width)+sx)as usize];
                }
            }
        }
        let labels=["Disconnect","Fullscreen",if !self.allow_input{"View only"}else if self.input_enabled{"Pause input"}else{"Enable input"},if self.screens>1{if self.switch_pending{"Switching..."}else{"Next screen"}}else{""},if !self.extended{""}else if self.quality_pending.is_some(){"Changing..."}else{match self.quality_mode{0=>"Quality Auto",1=>"Quality Full",2=>"Quality Mid",_=>"Quality Low"}}];
        for(i,label)in labels.iter().enumerate(){
            let x=i as u32*cell;if x>=w||label.is_empty(){continue;}
            for y in 0..self.toolbar{for xx in x..(x+cell-1).min(w){buffer[(y*w+xx)as usize]=if i==0{0x742d32}else{0x263441};}}
            draw_text(&mut buffer,w,h,x+5*scale,9*scale,label,scale);
        }
        let summary=if let Some(i)=self.view_index{if i==self.screens{format!("All | {:.0} fps",self.fps)}else{format!("{}/{} | {:.0} fps",i+1,self.screens,self.fps)}}else{format!("{:.0} fps",self.fps)};
        let offset=if self.extended{5}else{4};
        if offset*cell<w{draw_text(&mut buffer,w,h,offset*cell+5*scale,9*scale,&summary,scale);}
        buffer.present().map_err(error)
    }
    fn fail(&mut self,el:&ActiveEventLoop,e:String){self.error=Some(e);el.exit();}
}
fn draw_text(buf:&mut[u32],width:u32,height:u32,x:u32,y:u32,text:&str,scale:u32){
    for(i,ch)in text.chars().enumerate(){
        if let Some(glyph)=BASIC_FONTS.get(ch){
            for (row,bits)in glyph.iter().enumerate(){for col in 0..8u32{
                if bits&(1u8<<col)==0{continue;}
                for dy in 0..scale{for dx in 0..scale{
                    let xx=x+i as u32*8*scale+col*scale+dx;let yy=y+row as u32*scale+dy;
                    if xx<width&&yy<height{buf[(yy*width+xx)as usize]=0xf2f5f8;}
                }}
            }}
        }
    }
}
impl ApplicationHandler for Viewer<'_>{
    fn resumed(&mut self,el:&ActiveEventLoop){
        if self.window.is_some(){return;}
        let c=self.peer.config();
        let result:MediaResult<()>= (||{
            let window=Rc::new(el.create_window(Window::default_attributes().with_title("Remote EX - Esc disconnects, F11 fullscreen")
                .with_inner_size(LogicalSize::new(c.width.min(1440),c.height.min(900)+32))
                .with_min_inner_size(LogicalSize::new(450u32,180u32))).map_err(error)?);
            let context=softbuffer::Context::new(window.clone()).map_err(error)?;
            let surface=Surface::new(&context,window.clone()).map_err(error)?;
            let size=window.inner_size();
            self.window=Some(window);self.surface=Some(surface);self.layout(size)?;
            Ok(())
        })();
        if let Err(e)=result{self.fail(el,e);}
    }
    fn window_event(&mut self,el:&ActiveEventLoop,id:WindowId,event:WindowEvent){
        if self.window.as_ref().is_none_or(|w|w.id()!=id){return;}
        let result:MediaResult<()>= (||{
            match event{
                WindowEvent::CloseRequested=>el.exit(),
                WindowEvent::Resized(size)=>self.layout(size)?,
                WindowEvent::ScaleFactorChanged{..}=>{let size=self.window.as_ref().unwrap().inner_size();self.layout(size)?;}
                WindowEvent::Focused(focused)=>{self.disarm()?;self.focused=focused;self.physical_keys.clear();self.physical_buttons.clear();self.last_cursor=None;}
                WindowEvent::CursorLeft{..}=>{self.last_cursor=None;self.disarm()?;}
                WindowEvent::CursorMoved{position,..}=>{self.last_cursor=Some((position.x,position.y));self.reconcile()?;}
                WindowEvent::MouseInput{state,button,..}=>{
                    if let Some((x,y))=self.last_cursor{
                        if y>=0.0&&y<f64::from(self.toolbar){
                            self.disarm()?;
                            if state==ElementState::Pressed&&button==MouseButton::Left&&self.focused&&x>=0.0{
                                self.local_action((x as u32)/(112*self.scale),el)?;
                            }
                            let code=match button{MouseButton::Left=>1,MouseButton::Middle=>2,MouseButton::Right=>3,_=>0};
                            if code!=0{if state==ElementState::Pressed{self.physical_buttons.insert(code);}else{self.physical_buttons.remove(&code);}}
                            return Ok(());
                        }
                    }
                    let code=match button{MouseButton::Left=>1,MouseButton::Middle=>2,MouseButton::Right=>3,_=>0};
                    if code!=0{if state==ElementState::Pressed{self.physical_buttons.insert(code);}else{self.physical_buttons.remove(&code);}self.reconcile()?;}
                }
                WindowEvent::KeyboardInput{event,is_synthetic,..}=>{
                    if let PhysicalKey::Code(code)=event.physical_key{
                        if event.state==ElementState::Pressed&&!event.repeat&&!is_synthetic&&self.focused{
                            if code==K::Escape{self.disarm()?;el.exit();return Ok(());}
                            if code==K::F11{self.local_action(1,el)?;return Ok(());}
                            if code==K::F9&&self.extended{self.local_action(4,el)?;return Ok(());}
                        }
                        if code!=K::Escape&&code!=K::F11&&!(code==K::F9&&self.extended){
                            if let Some(key)=hid(code){
                                if event.state==ElementState::Pressed{self.physical_keys.insert(key);}else{self.physical_keys.remove(&key);}
                                if !is_synthetic{self.reconcile()?;}
                            }
                        }
                    }
                }
                WindowEvent::MouseWheel{delta,..} if self.armed&&self.focused&&self.pointer().is_some()=>{
                    let(x,y)=match delta{MouseScrollDelta::LineDelta(x,y)=>(x as f64,y as f64),MouseScrollDelta::PixelDelta(p)=>(p.x/40.0,p.y/40.0)};
                    let units=|v:f64|if v.is_finite(){(v*120.).round().clamp(-1200.,1200.)as i32}else{0};
                    let(x,y)=(units(x),units(-y));if x!=0||y!=0{self.emit(Event::Scroll(Scroll{x,y}))?;}
                }
                WindowEvent::RedrawRequested=>self.paint()?,
                _=>{},
            }
            Ok(())
        })();
        if let Err(e)=result{self.fail(el,e);}
    }
    fn about_to_wait(&mut self,el:&ActiveEventLoop){
        if self.error.is_some(){el.exit();return;}
        let now=Instant::now();
        if now>=self.next_frame{
            if let Err(e)=self.poll(el){self.fail(el,e);return;}
            self.next_frame=Instant::now()+Duration::from_millis(16);
        }
        el.set_control_flow(ControlFlow::WaitUntil(self.next_frame));
    }
}
pub(super) fn run(peer:&Peer,channel:&ControlChannel,session:[u8;16],geometry:Geometry,allow_input:bool,screens:u8,av:bool,extended:bool)->MediaResult<(u32,bool)>{
    let c=peer.config();let now=Instant::now();
    let event_loop:EventLoop<()>=EventLoop::new().map_err(error)?;
    let mut v=Viewer{peer,channel,session,geometry:Rect{x:geometry.x,y:geometry.y,width:geometry.width,height:geometry.height},
        allow_input,input_enabled:allow_input,av,volume:audio::VOLUME.load(Ordering::Acquire),screens,view_index:None,switch_pending:false,window:None,surface:None,size:PhysicalSize::new(0,0),
        toolbar:0,scale:1,fit:None,last_cursor:None,focused:false,armed:false,physical_keys:BTreeSet::new(),physical_buttons:BTreeSet::new(),
        sent_keys:BTreeSet::new(),sent_buttons:BTreeSet::new(),previous:None,seq:0,pixels:vec![0;(c.width*c.height)as usize],
        frames:0,metric_frames:0,metric_bytes:0,last_metric:now,last_frame:now,stats:Default::default(),started:now,
        heartbeat:now-Duration::from_secs(1),released:false,error:None,next_frame:now,fps:0.,extended,quality_mode:0,quality_pending:None,actual_stream:None};
    let result=event_loop.run_app(&mut v).map_err(error);
    let release=v.disarm();
    result?;release?;
    if let Some(e)=v.error{return Err(e);}
    Ok((v.frames,v.released))
}
