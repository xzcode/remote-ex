//! Pure native-viewer localization/layout data. No remote message supplies
//! locale, labels, or local toolbar hit-testing geometry.
use crate::viewport::Rect;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
#[repr(u8)]
pub enum Locale{En,Zh,De,Fr,Es,Pt,Ja,Ko}
impl Locale{
    pub fn parse(s:&str)->Option<Self>{Some(match s{"en"=>Self::En,"zh"=>Self::Zh,"de"=>Self::De,"fr"=>Self::Fr,"es"=>Self::Es,"pt"=>Self::Pt,"ja"=>Self::Ja,"ko"=>Self::Ko,_=>return None})}
    pub fn code(self)->&'static str{match self{Self::En=>"en",Self::Zh=>"zh",Self::De=>"de",Self::Fr=>"fr",Self::Es=>"es",Self::Pt=>"pt",Self::Ja=>"ja",Self::Ko=>"ko"}}
    pub fn from_index(i:u8)->Self{match i{1=>Self::Zh,2=>Self::De,3=>Self::Fr,4=>Self::Es,5=>Self::Pt,6=>Self::Ja,7=>Self::Ko,_=>Self::En}}
    pub fn title(self)->&'static str{match self{
        Self::En=>"Remote EX — Esc disconnects · F11 fullscreen",
        Self::Zh=>"Remote EX — Esc 断开 · F11 全屏 · Pause input 暂停输入 · Next screen 下一屏 · Quality 画质",
        Self::De=>"Remote EX — Esc trennt · F11 Vollbild",
        Self::Fr=>"Remote EX — Échap déconnecte · F11 plein écran",
        Self::Es=>"Remote EX — Esc desconecta · F11 pantalla completa",
        Self::Pt=>"Remote EX — Esc desconecta · F11 tela cheia",
        Self::Ja=>"Remote EX — Esc 切断 · F11 全画面 · Pause input 入力停止 · Next screen 次の画面 · Quality 画質",
        Self::Ko=>"Remote EX — Esc 연결 해제 · F11 전체 화면 · Pause input 입력 중지 · Next screen 다음 화면 · Quality 화질",
    }}
    /// Compact ASCII toolbar strings for the existing public-domain bitmap
    /// font. CJK keeps the English labels legible and uses the Unicode OS title
    /// for the translated legend; native CJK glyph rendering is still pending.
    pub fn labels(self)->[&'static str;12]{match self{
        Self::De=>["Trennen","Vollbild","Nur Ansicht","Eingabe aus","Eingabe an","Bildschirm","Wechsel...","Qual. Auto","Qual. Hoch","Qual. Mittel","Qual. Niedr.","Wechsel..."],
        Self::Fr=>["Deconnecter","Plein ecran","Vue seule","Pause saisie","Activer","Ecran suivant","Changement","Qual. Auto","Qual. Haute","Qual. Moyen","Qual. Basse","Changement"],
        Self::Es=>["Desconectar","Pant. compl.","Solo vista","Pausar entrada","Activar","Otra pantalla","Cambiando","Cal. Auto","Cal. Alta","Cal. Media","Cal. Baja","Cambiando"],
        Self::Pt=>["Desconectar","Tela cheia","So ver","Pausar entrada","Ativar","Proxima tela","Mudando","Qual. Auto","Qual. Alta","Qual. Media","Qual. Baixa","Mudando"],
        _=>["Disconnect","Fullscreen","View only","Pause input","Enable input","Next screen","Switching...","Quality Auto","Quality Full","Quality Mid","Quality Low","Changing..."],
    }}
}
#[derive(Clone,Debug)]
pub struct Toolbar{pub actions:[Option<Rect>;5],pub summary:Option<Rect>,pub height:u32}
pub fn toolbar(width:u32,height:u32,scale:u32,switch:bool,extended:bool)->Toolbar{
    let scale=scale.clamp(1,3);
    let mut result=Toolbar{actions:[None;5],summary:None,height:0};
    if width==0||height==0{return result;}
    let ids:Vec<usize>=[0,1,2,3,4].into_iter().filter(|i|match i{3=>switch,4=>extended,_=>true}).collect();
    let cols=(width/(136*scale)).clamp(1,ids.len()as u32);
    let row_height=32*scale;
    let rows=(ids.len()as u32).div_ceil(cols);
    for (n,id) in ids.into_iter().enumerate(){
        let n=n as u32;let col=n%cols;let row=n/cols;let y=row*row_height;
        if y>=height{continue;}
        let x=(u64::from(col)*u64::from(width)/u64::from(cols))as u32;
        let right=(u64::from(col+1)*u64::from(width)/u64::from(cols))as u32;
        if right>x{result.actions[id]=Some(Rect{x,y,width:right-x,height:row_height.min(height-y)});}
    }
    let y=(rows*row_height).min(height);
    if y<height{result.summary=Some(Rect{x:0,y,width,height:(20*scale).min(height-y)});}
    result.height=(y+20*scale).min(height);
    result
}
impl Toolbar{
    pub fn hit(&self,x:f64,y:f64)->Option<u32>{
        if !x.is_finite()||!y.is_finite(){return None;}
        self.actions.iter().enumerate().find_map(|(i,r)|{
            let r=r.as_ref()?;
            (x>=f64::from(r.x)&&x<f64::from(r.x+r.width)&&y>=f64::from(r.y)&&y<f64::from(r.y+r.height)).then_some(i as u32)
        })
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn all_controls_reachable_on_narrow_dpi_windows(){
        for scale in 1..=3{
            let w=450*scale;let h=180*scale;
            let t=toolbar(w,h,scale,true,true);
            assert!(t.height<h);
            for(i,r)in t.actions.iter().enumerate(){let r=r.unwrap();assert_eq!(t.hit(f64::from(r.x)+1.,f64::from(r.y)+1.),Some(i as u32));}
            let r=t.summary.unwrap();assert_eq!(t.hit(1.,f64::from(r.y)+1.),None);
            assert_eq!(t.hit(1.,f64::from(t.height)),None);
        }
    }
    #[test]fn unavailable_actions_and_invalid_hits_are_inert(){
        let t=toolbar(800,400,1,false,false);assert!(t.actions[3].is_none()&&t.actions[4].is_none());
        assert_eq!(t.hit(f64::NAN,1.),None);assert_eq!(t.hit(-1.,1.),None);
        let t=toolbar(0,0,1,true,true);assert_eq!(t.height,0);
    }
    #[test]fn locales_are_closed_and_bitmap_safe(){
        for i in 0..8{let l=Locale::from_index(i);assert_eq!(Locale::parse(l.code()),Some(l));assert!(l.labels().iter().all(|s|s.is_ascii()&&s.len()<=15));}
        assert!(Locale::parse("remote-string").is_none());
    }
}
