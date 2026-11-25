//! Physical-pixel geometry shared by the software renderer and local input.
//! The local toolbar and letterboxing are never part of the remote input area.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct Rect{pub x:u32,pub y:u32,pub width:u32,pub height:u32}
pub fn fit(canvas:(u32,u32),area:Rect)->Option<Rect>{
    let(w,h)=canvas;
    if w==0||h==0||area.width==0||area.height==0{return None;}
    let(dw,dh)=if u64::from(w)*u64::from(area.height)>=u64::from(h)*u64::from(area.width){
        (area.width,(u64::from(h)*u64::from(area.width)/u64::from(w)).max(1)as u32)
    }else{((u64::from(w)*u64::from(area.height)/u64::from(h)).max(1)as u32,area.height)};
    Some(Rect{x:area.x.checked_add((area.width-dw)/2)?,y:area.y.checked_add((area.height-dh)/2)?,width:dw,height:dh})
}
pub fn map_pointer(point:(f64,f64),fit:Rect,canvas:(u32,u32),content:Rect)->Option<(u32,u32)>{
    let (x,y)=point;
    if !x.is_finite()||!y.is_finite()||fit.width<2||fit.height<2||content.width<2||content.height<2||canvas.0<2||canvas.1<2{return None;}
    if content.x.checked_add(content.width)?>canvas.0||content.y.checked_add(content.height)?>canvas.1{return None;}
    if x<f64::from(fit.x)||x>=f64::from(fit.x.checked_add(fit.width)?)||y<f64::from(fit.y)||y>=f64::from(fit.y.checked_add(fit.height)?){return None;}
    let px=(x-f64::from(fit.x))*f64::from(canvas.0)/f64::from(fit.width);
    let py=(y-f64::from(fit.y))*f64::from(canvas.1)/f64::from(fit.height);
    if px<f64::from(content.x)||px>=f64::from(content.x+content.width)||py<f64::from(content.y)||py>=f64::from(content.y+content.height){return None;}
    let norm=|v:f64,offset:u32,size:u32|(((v-f64::from(offset))/f64::from(size-1)).clamp(0.0,1.0)*65535.0).round()as u32;
    Some((norm(px,content.x,content.width),norm(py,content.y,content.height)))
}
/// Equal grid tiles, in the order explicitly authorized by the host.
pub fn tiles(width:u32,height:u32,count:usize)->Option<Vec<Rect>>{
    if !(1..=4).contains(&count)||width<4||height<4{return None;}
    let cols=if count==1{1}else{2};let rows=if count<=2{1}else{2};
    Some((0..count).map(|i|{
        let c=i as u32%cols;let r=i as u32/cols;
        let x=c*width/cols;let y=r*height/rows;
        Rect{x,y,width:(c+1)*width/cols-x,height:(r+1)*height/rows-y}
    }).collect())
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn toolbar_and_double_letterboxing_excluded(){
        let fitted=fit((960,540),Rect{x:0,y:40,width:1000,height:800}).unwrap();
        let content=Rect{x:120,y:0,width:720,height:540};
        assert!(map_pointer((500.,20.),fitted,(960,540),content).is_none());
        assert!(map_pointer((5.,400.),fitted,(960,540),content).is_none());
        let (x,y)=map_pointer((500.,440.),fitted,(960,540),content).unwrap();
        assert!((32000..34000).contains(&x)&&(32000..34000).contains(&y));
        assert!(map_pointer((f64::NAN,0.),fitted,(960,540),content).is_none());
    }
    #[test]fn mosaic_is_bounded_and_nonoverlapping(){
        let t=tiles(961,541,4).unwrap();
        assert_eq!(t.iter().map(|r|r.width*r.height).sum::<u32>(),961*541);
        assert_eq!(t[0].x+t[0].width,t[1].x);
        assert_eq!(t[0].y+t[0].height,t[2].y);
        assert!(tiles(960,540,5).is_none());
    }
}
