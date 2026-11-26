//! Rasterize only fixed local toolbar labels using installed system fonts.
//! Work occurs off the viewer event loop; media/input never wait for fonts.
//! No downloaded/embedded fonts, remote strings, filesystem paths from peers,
//! or font bytes in logs, output, or artifacts.
use ab_glyph::{Font,FontRef,ScaleFont,point};
use fontdb::{Database,ID};
use remote_ex_desktop_features::{viewport::Rect,viewer_ui::Locale};
use std::sync::mpsc::{Receiver,sync_channel};
const MAX_FONT_BYTES:usize=64*1024*1024;
const W:u32=136;
const H:u32=24;
struct Tile{width:u32,height:u32,alpha:Vec<u8>}
pub(super) struct Text{tiles:Vec<Tile>}
fn raster(font:&FontRef<'_>,label:&str,scale:u32)->Option<Tile>{
    let width=W*scale;let height=H*scale;
    let mut tile=Tile{width,height,alpha:vec![0;(width*height)as usize]};
    let s=font.as_scaled(16.0*scale as f32);
    let mut advance=0.0f32;
    for ch in label.chars(){let id=font.glyph_id(ch);if id.0==0{return None;}advance+=s.h_advance(id);}
    if !advance.is_finite()||advance<=0.0||advance>width as f32-4.0{return None;}
    let baseline=(height as f32-s.height())/2.0+s.ascent();
    if !baseline.is_finite(){return None;}
    let mut pen=(width as f32-advance)/2.0;
    let mut any=false;
    for ch in label.chars(){
        let id=font.glyph_id(ch);
        if !ch.is_whitespace(){
            let glyph=id.with_scale_and_position(16.0*scale as f32,point(pen,baseline));
            let outlined=font.outline_glyph(glyph)?;
            let b=outlined.px_bounds();
            if !b.min.x.is_finite()||!b.min.y.is_finite()||!b.max.x.is_finite()||!b.max.y.is_finite()||
                b.width()>width as f32||b.height()>height as f32||b.min.x<0.0||b.min.y<0.0||b.max.x>width as f32||b.max.y>height as f32{return None;}
            let mut glyph_visible=false;
            outlined.draw(|x,y,c|{
                let px=b.min.x as u32+x;let py=b.min.y as u32+y;
                if px<width&&py<height&&c.is_finite(){
                    let a=(c.clamp(0.,1.)*255.).round()as u8;
                    let index=(py*width+px)as usize;
                    tile.alpha[index]=tile.alpha[index].max(a);
                    glyph_visible|=a!=0;
                }
            });
            if !glyph_visible{return None;}any=true;
        }
        pen+=s.h_advance(id);
    }
    any.then_some(tile)
}
fn preferred(locale:Locale,name:&str)->usize{
    let name=name.to_ascii_lowercase();
    let candidates:&[&str]=match locale{
        Locale::Zh=>&["microsoft yahei","pingfang sc","noto sans cjk sc","source han sans sc","simhei","simsun"],
        Locale::Ja=>&["yu gothic","hiragino sans","noto sans cjk jp","source han sans jp","meiryo"],
        Locale::Ko=>&["malgun gothic","apple sd gothic neo","noto sans cjk kr","source han sans kr"],
        _=>&["segoe ui","arial","helvetica","noto sans","dejavu sans","liberation sans"],
    };
    candidates.iter().position(|s|name.contains(*s)).unwrap_or(100)
}
fn build(locale:Locale)->Option<Text>{
    let mut db=Database::new();db.load_system_fonts();
    let mut faces:Vec<(usize,ID)>=db.faces().map(|f|(f.families.iter().map(|(name,_)|preferred(locale,name)).min().unwrap_or(100),f.id)).collect();
    faces.sort_by_key(|x|x.0);
    let labels=locale.localized_labels();
    // Try preferred regional families first, with a finite candidate limit.
    for (_,id) in faces.into_iter().take(128){
        let result=db.with_face_data(id,|bytes,index|{
            if bytes.is_empty()||bytes.len()>MAX_FONT_BYTES{return None;}
            let font=FontRef::try_from_slice_and_index(bytes,index).ok()?;
            if labels.iter().flat_map(|s|s.chars()).any(|ch|font.glyph_id(ch).0==0){return None;}
            let mut tiles=Vec::with_capacity(36);
            for scale in 1..=3{for label in labels{tiles.push(raster(&font,label,scale)?);}}
            Some(Text{tiles})
        }).flatten();
        if result.is_some(){return result;}
    }
    None
}
pub(super) fn start(locale:Locale)->Receiver<Option<Text>>{
    let (tx,rx)=sync_channel(1);
    // A missing font or slow discovery cannot block a connected session.
    std::thread::spawn(move||{let result=std::panic::catch_unwind(||build(locale)).ok().flatten();let _=tx.send(result);});
    rx
}
impl Text{
    pub(super) fn draw(&self,buf:&mut[u32],width:u32,height:u32,rect:Rect,label:usize,scale:u32)->bool{
        let Some(tile)=self.tiles.get(((scale.clamp(1,3)-1)*12)as usize+label)else{return false};
        if rect.width<tile.width||rect.height<tile.height||width==0||height==0{return false;}
        let ox=rect.x+(rect.width-tile.width)/2;let oy=rect.y+(rect.height-tile.height)/2;
        for y in 0..tile.height{
            for x in 0..tile.width{
                let a=u32::from(tile.alpha[(y*tile.width+x)as usize]);if a==0{continue;}
                let px=ox+x;let py=oy+y;
                if px>=width||py>=height{continue;}
                let Some(pixel)=buf.get_mut((u64::from(py)*u64::from(width)+u64::from(px))as usize)else{continue};
                let blend=|shift:u32|(((((*pixel>>shift)&255)*(255-a)+244*a+127)/255)&255)<<shift;
                *pixel=blend(16)|blend(8)|blend(0);
            }
        }true
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn installed_cjk_font_fixture(){
        // Only CI explicitly requiring Noto CJK treats missing system fonts as
        // failure. On ordinary developer machines the fallback remains valid.
        if std::env::var("REMOTE_EX_REQUIRE_CJK_FONT").ok().as_deref()!=Some("1"){return;}
        for locale in [Locale::Zh,Locale::Ja,Locale::Ko]{
            let text=build(locale).expect("required system CJK font not usable");
            for scale in 1..=3{
                let mut b=vec![0;((160*scale)*(32*scale))as usize];
                assert!(text.draw(&mut b,160*scale,32*scale,Rect{x:0,y:0,width:160*scale,height:32*scale},0,scale));
                assert!(b.iter().any(|p|*p!=0));
            }
        }
    }
}
