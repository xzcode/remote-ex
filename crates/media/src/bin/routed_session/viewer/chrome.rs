//! Local product chrome; no remote text, pixels or input actions.
use super::{native_text,draw_text,draw_mark,fill_rect};
use remote_ex_desktop_features::{viewport::Rect,viewer_ui::{Locale,Toolbar}};

#[derive(Clone,Copy)]
struct Palette{bar:u32,line:u32,button:u32,hover:u32,disabled:u32,text:u32,muted:u32,accent:u32,active:u32,active_hover:u32,active_text:u32,red:u32,red_hover:u32,dot:u32}
fn palette(dark:bool)->Palette{
    if dark{Palette{bar:0x202733,line:0x394454,button:0x2b3544,hover:0x3b4c61,disabled:0x252c36,text:0xe6edf7,muted:0xa5b2c4,accent:0x87b4ed,active:0x284b3e,active_hover:0x315e4b,active_text:0x9ce3bb,red:0x963844,red_hover:0xb74954,dot:0x55c38e}}
    else{Palette{bar:0xf5f7fa,line:0xd8e0ea,button:0xffffff,hover:0xe8eff8,disabled:0xecf0f4,text:0x293b50,muted:0x65758a,accent:0x3465ad,active:0xe2f1e9,active_hover:0xd1eadc,active_text:0x216345,red:0xb3404c,red_hover:0x99313d,dot:0x26966a}}
}
pub(super) struct State<'a>{
    pub locale:Locale,pub dark:bool,pub allow_input:bool,pub input_enabled:bool,pub switching:bool,
    pub quality:u8,pub changing:bool,pub fullscreen:bool,pub hover:Option<u32>,
    pub frames:u32,pub dimensions:Option<(u32,u32)>,pub fps:f64,pub route:Option<&'a str>,
    pub rtt:Option<u32>,pub bitrate:Option<u64>,pub display:Option<(u8,u8)>,
}
fn rounded(buf:&mut[u32],w:u32,h:u32,r:Rect,radius:u32,color:u32){
    let radius=radius.min(r.width/2).min(r.height/2);
    for dy in 0..r.height{
        let d=if dy<radius{radius-dy}else if dy>=r.height-radius{dy-(r.height-radius)+1}else{0};
        let cut=if d==0{0}else{radius-((f64::from(radius*radius-d*d)).sqrt()as u32)};
        fill_rect(buf,w,h,Rect{x:r.x+cut,y:r.y+dy,width:r.width.saturating_sub(2*cut),height:1},color);
    }
}
fn inset(r:Rect,n:u32)->Rect{Rect{x:r.x+n.min(r.width/2),y:r.y+n.min(r.height/2),width:r.width.saturating_sub(2*n),height:r.height.saturating_sub(2*n)}}
fn text(buf:&mut[u32],w:u32,h:u32,r:Rect,s:&str,scale:u32,color:u32,font:Option<&native_text::Text>){
    if r.width==0||r.height==0{return;}
    let count=(r.width/(10*scale))as usize;let value=&s[..s.len().min(count)];
    if font.is_some_and(|f|f.draw_ascii(buf,w,h,r,value,scale,color)){return;}
    let count=(r.width/(8*scale))as usize;
    draw_text(buf,w,h,r.x,r.y+(r.height.saturating_sub(8*scale))/2,&s[..s.len().min(count)],scale,color);
}
pub(super) fn paint(buf:&mut[u32],w:u32,h:u32,bar:&Toolbar,scale:u32,font:Option<&native_text::Text>,s:&State<'_>){
    let p=palette(s.dark);let scale=scale.clamp(1,3);
    fill_rect(buf,w,h,Rect{x:0,y:0,width:w,height:bar.height},p.bar);
    if bar.height>0{fill_rect(buf,w,h,Rect{x:0,y:bar.height-1,width:w,height:1},p.line);}
    if bar.footer_height>0{
        let y=h.saturating_sub(bar.footer_height);fill_rect(buf,w,h,Rect{x:0,y,width:w,height:bar.footer_height},p.bar);
        fill_rect(buf,w,h,Rect{x:0,y,width:w,height:1},p.line);
    }
    if let Some(r)=bar.brand{
        let mark=Rect{x:r.x,y:r.y+(r.height.saturating_sub(30*scale))/2,width:30*scale,height:30*scale};
        rounded(buf,w,h,mark,6*scale,0x3465ad);
        draw_mark(buf,w,h,mark.x+9*scale,mark.y+9*scale,3,scale,0xffffff);
        let label=Rect{x:mark.x+39*scale,y:r.y,width:r.width.saturating_sub(39*scale),height:r.height};
        text(buf,w,h,label,"Remote EX",scale,p.text,font);
    }
    let indices=[0,1,if !s.allow_input{2}else if s.input_enabled{3}else{4},if s.switching{6}else{5},if s.changing{11}else{match s.quality{0=>7,1=>8,2=>9,_=>10}}];
    let labels=s.locale.labels();
    for(i,r)in bar.actions.iter().enumerate(){
        let Some(r)=r else{continue};
        let enabled=match i{2=>s.allow_input,3=>!s.switching,4=>!s.changing,_=>true};
        let hovered=enabled&&s.hover==Some(i as u32);
        let (border,bg,fg)=match(i,enabled,hovered){
            (0,_,true)=>(p.red_hover,p.red_hover,0xffffff),(0,_,_)=>(p.red,p.red,0xffffff),
            (2,true,true) if s.input_enabled=>(p.active_text,p.active_hover,p.active_text),
            (2,true,_) if s.input_enabled=>(p.active,p.active,p.active_text),
            (_,false,_)=>(p.line,p.disabled,p.muted),
            (_,_,true)=>(p.accent,p.hover,p.text),_=>(p.line,p.button,p.text)
        };
        rounded(buf,w,h,*r,5*scale,border);
        rounded(buf,w,h,inset(*r,scale),4*scale,bg);
        let mut label_rect=*r;
        if r.width>=166*scale&&r.height>=26*scale{
            draw_mark(buf,w,h,r.x+10*scale,r.y+(r.height-12*scale)/2,i,scale,if i==0{0xffffff}else if enabled{p.accent}else{p.muted});
            label_rect.x+=26*scale;label_rect.width-=26*scale;
        }
        // Fullscreen uses the same reversible action; the OS window state
        // remains authoritative. No separate unimplemented button.
        if i==1&&s.fullscreen{
            fill_rect(buf,w,h,Rect{x:r.x+4*scale,y:r.y+4*scale,width:3*scale,height:3*scale},p.accent);
        }
        if font.is_some_and(|f|f.draw_tinted(buf,w,h,label_rect,indices[i],scale,fg)){continue;}
        let label=labels[indices[i]];
        let width=(label.len()as u32)*8*scale;
        let max=(label_rect.width/(8*scale))as usize;
        draw_text(buf,w,h,label_rect.x+label_rect.width.saturating_sub(width)/2,label_rect.y+label_rect.height.saturating_sub(8*scale)/2,&label[..label.len().min(max)],scale,fg);
    }
    if let Some(r)=bar.summary{
        let dim=s.dimensions.map(|(x,y)|format!("{x}x{y}")).unwrap_or_else(||"- x -".into());
        let fps=if s.fps.is_finite()&&s.fps>0.0{format!("{:.0} FPS",s.fps)}else{"- FPS".into()};
        let route=match s.route{Some("direct")=>"P2P",Some("relay")=>"TURN",_=>"-"};
        let rtt=s.rtt.map(|v|format!("{v} ms")).unwrap_or_else(||"- ms".into());
        let rate=s.bitrate.map(|v|format!("{:.1} Mb/s",v as f64/1_000_000.)).unwrap_or_else(||"- Mb/s".into());
        let display=s.display.map(|(i,n)|if i==n{"All".into()}else{format!("{}/{}",i+1,n)}).unwrap_or_else(||"-".into());
        let live=s.frames>0;
        let status=if live{"LIVE"}else{"WAIT"};
        rounded(buf,w,h,Rect{x:r.x+2*scale,y:r.y+r.height.saturating_sub(8*scale)/2,width:8*scale,height:8*scale},4*scale,if live{p.dot}else{p.muted});
        let mut x=r.x+16*scale;let end=r.x+r.width;
        let fields=[status.to_owned(),dim,fps,route.into(),rtt,rate,display];
        for (i,field) in fields.iter().enumerate(){
            let width=(field.len()as u32)*10*scale;
            if x+width>end{break;}
            text(buf,w,h,Rect{x,y:r.y,width,height:r.height},field,scale,if i==0&&live{p.dot}else if i==0{p.muted}else{p.text},font);
            x+=width+10*scale;
            if i+1<fields.len()&&x+1<end{
                fill_rect(buf,w,h,Rect{x,y:r.y+8*scale,width:1,height:r.height.saturating_sub(16*scale)},p.line);
                x+=11*scale;
            }
        }
        let hint="Esc / F11";
        let hint_width=hint.len()as u32*10*scale;
        if end>=x+hint_width+12*scale{text(buf,w,h,Rect{x:end-hint_width,y:r.y,width:hint_width,height:r.height},hint,scale,p.muted,font);}
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    use remote_ex_desktop_features::viewer_ui;
    #[test]
    fn product_chrome_preview(){
        let(w,h)=(1280,800);let bar=viewer_ui::toolbar(w,h,1,true,true);
        let font=native_text::start(Locale::En).recv_timeout(std::time::Duration::from_secs(20)).ok().flatten();
        for dark in [false,true]{
            let mut pixels=vec![0x101722;(w*h)as usize];
            let s=State{locale:Locale::En,dark,allow_input:false,input_enabled:false,switching:false,quality:0,changing:false,fullscreen:false,hover:Some(1),frames:0,dimensions:None,fps:0.,route:None,rtt:None,bitrate:None,display:None};
            let area=bar.picture_area(w,h);let picture=remote_ex_desktop_features::viewport::fit((960,540),area).unwrap();
            // Generated, deliberately abstract desktop geometry. This test
            // cannot capture the screen, access a peer or start a session.
            fill_rect(&mut pixels,w,h,picture,0xdfe7f1);
            fill_rect(&mut pixels,w,h,Rect{x:picture.x,y:picture.y,width:picture.width,height:36},0xeaf0f8);
            let panel=Rect{x:picture.x+picture.width/8,y:picture.y+picture.height/5,width:picture.width*3/4,height:picture.height*3/5};
            rounded(&mut pixels,w,h,panel,9,0xf8fafc);
            for k in 0..5{fill_rect(&mut pixels,w,h,Rect{x:panel.x+24,y:panel.y+32+k*29,width:panel.width/2+k*12,height:5},0xd2deed);}
            draw_text(&mut pixels,w,h,panel.x+24,panel.y+panel.height-40,"GENERATED PREVIEW - NO REMOTE SESSION",1,0x52667d);
            paint(&mut pixels,w,h,&bar,1,font.as_ref(),&s);
            let off=|r:Rect|((r.y+r.height/2)*w+r.x+r.width/2)as usize;
            assert_ne!(pixels[0],pixels[off(picture)]);
            assert_ne!(pixels[off(bar.actions[0].unwrap())],pixels[off(bar.actions[1].unwrap())]);
            if let Ok(dir)=std::env::var("REMOTE_EX_VIEWER_PREVIEW_DIR"){
                assert_eq!(std::env::var("RUNNER_ENVIRONMENT").as_deref(),Ok("github-hosted"));
                use std::io::Write;
                let name=if dark{"pc-session-dark.ppm"}else{"pc-session-light.ppm"};
                let f=std::fs::File::create(std::path::Path::new(&dir).join(name)).unwrap();let mut out=std::io::BufWriter::new(f);
                write!(out,"P6\n{w} {h}\n255\n").unwrap();
                for p in pixels{out.write_all(&[(p>>16)as u8,(p>>8)as u8,p as u8]).unwrap();}
                out.flush().unwrap();
            }
        }
    }
}
