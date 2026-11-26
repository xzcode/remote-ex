//! Locally requested pause of native screen-only viewing. It neither turns
//! off the physical monitor nor allows hidden control. A fresh epoch blocks
//! buffered frames from a pre-pause capture from being published on resume.
use super::*;
use font8x8::{UnicodeFonts,BASIC_FONTS};
static EPOCH:AtomicUsize=AtomicUsize::new(0);
pub(super) fn epoch()->usize{EPOCH.load(Ordering::Acquire)}
pub(super) fn paused(epoch:usize)->bool{epoch%2==1}
pub(in crate::routed_session) fn change(pause:bool)->MediaResult<()>{
    EPOCH.fetch_update(Ordering::AcqRel,Ordering::Acquire,|old|{
        if old>=128||paused(old)==pause{None}else{old.checked_add(1)}
    }).map_err(|_|"Invalid local sharing-pause transition")?;Ok(())
}
pub(super) fn blank(config:VideoConfig)->Vec<u8>{
    let mut image=vec![0u8;config.rgba_bytes()];
    for pixel in image.chunks_exact_mut(4){pixel[3]=255;}
    let label="PAUSED BY HOST";
    let scale=(config.width/(label.len()as u32*9)).clamp(1,5);
    let width=label.len()as u32*9*scale;
    let height=8*scale;
    if width>config.width||height>config.height{return image;}
    let left=(config.width-width)/2;let top=(config.height-height)/2;
    for (i,ch) in label.chars().enumerate(){
        if let Some(rows)=BASIC_FONTS.get(ch){
            for (y,row) in rows.into_iter().enumerate(){for x in 0..8u32{
                if row&(1u8<<x)==0{continue;}
                for sy in 0..scale{for sx in 0..scale{
                    let px=left+i as u32*9*scale+x*scale+sx;let py=top+y as u32*scale+sy;
                    let at=((py*config.width+px)*4)as usize;image[at..at+4].copy_from_slice(&[220,220,220,255]);
                }}
            }}
        }
    }image
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn blank_contains_only_synthetic_pixels_and_status(){
        let c=VideoConfig{width:320,height:180,fps:10,bitrate:200_000};
        let b=blank(c);assert_eq!(b.len(),c.rgba_bytes());assert!(b.chunks_exact(4).all(|p|p==[0,0,0,255]||p==[220,220,220,255]));
        assert!(b.chunks_exact(4).any(|p|p[0]==220));assert!(paused(1)&&!paused(2));
    }
}
