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
    pub fn localized_labels(self)->[&'static str;12]{match self{
        Self::Zh=>["断开连接","全屏","仅查看","暂停输入","启用输入","下一屏","切换中…","画质 自动","画质 高","画质 中","画质 低","调整中…"],
        Self::Ja=>["切断","全画面","閲覧のみ","入力停止","入力開始","次の画面","切替中…","画質 自動","画質 高","画質 中","画質 低","変更中…"],
        Self::Ko=>["연결 해제","전체 화면","보기 전용","입력 중지","입력 활성화","다음 화면","전환 중…","화질 자동","화질 높음","화질 중간","화질 낮음","변경 중…"],
        Self::Fr=>["Déconnecter","Plein écran","Vue seule","Pause saisie","Activer","Écran suivant","Changement","Qual. Auto","Qual. Haute","Qual. Moyen","Qual. Basse","Changement"],
        Self::Pt=>["Desconectar","Tela cheia","Só ver","Pausar entrada","Ativar","Próxima tela","Mudando","Qual. Auto","Qual. Alta","Qual. Média","Qual. Baixa","Mudando"],
        _=>self.labels(),
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
pub struct Toolbar{pub actions:[Option<Rect>;5],pub summary:Option<Rect>,pub height:u32,pub footer_height:u32}
pub fn toolbar(width:u32,height:u32,scale:u32,switch:bool,extended:bool)->Toolbar{
    let scale=scale.clamp(1,3);
    let mut result=Toolbar{actions:[None;5],summary:None,height:0,footer_height:0};
    if width==0||height==0{return result;}
    // Actions retain their wire-independent IDs. Input, display and quality
    // precede fullscreen; the destructive disconnect button is always last.
    let ids:Vec<usize>=[2,3,4,1,0].into_iter().filter(|i|match i{3=>switch,4=>extended,_=>true}).collect();
    let edge=(8*scale).min(width/4);
    let gap=(5*scale).min(width/8);
    let inner=width.saturating_sub(2*edge);
    if inner==0{return result;}
    let cols=((inner+gap)/(136*scale+gap)).clamp(1,ids.len()as u32);
    let row_height=34*scale;
    let rows=(ids.len()as u32).div_ceil(cols);
    result.height=(rows*row_height+2*scale).min(height);
    result.footer_height=(23*scale).min(height-result.height);
    let available=inner.saturating_sub(gap*(cols-1));
    for (n,id) in ids.into_iter().enumerate(){
        let n=n as u32;let row=n/cols;let col=n%cols;
        let y=3*scale+row*row_height;
        if y>=result.height{continue;}
        let x=edge+(u64::from(col)*u64::from(available)/u64::from(cols))as u32+col*gap;
        let right=edge+(u64::from(col+1)*u64::from(available)/u64::from(cols))as u32+col*gap;
        if right>x{result.actions[id]=Some(Rect{x,y,width:right-x,height:(29*scale).min(result.height-y)});}
    }
    // Footer is independent of the top action strip and never accepts input.
    if result.footer_height>0{
        result.summary=Some(Rect{x:edge,y:height-result.footer_height,width:inner,height:result.footer_height});
    }
    result
}
impl Toolbar{
    pub fn picture_area(&self,width:u32,height:u32)->Rect{
        Rect{x:0,y:self.height.min(height),width,height:height.saturating_sub(self.height).saturating_sub(self.footer_height)}
    }
    pub fn hit(&self,x:f64,y:f64)->Option<u32>{
        if !x.is_finite()||!y.is_finite(){return None;}
        self.actions.iter().enumerate().find_map(|(i,r)|{
            let r=r.as_ref()?;
            (x>=f64::from(r.x)&&x<f64::from(r.x+r.width)&&y>=f64::from(r.y)&&y<f64::from(r.y+r.height)).then_some(i as u32)
        })
    }
    pub fn chrome(&self,y:f64,window_height:u32)->bool{
        y.is_finite()&&y>=0.0&&(y<f64::from(self.height)||y>=f64::from(window_height.saturating_sub(self.footer_height)))
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
            assert!(t.chrome(f64::from(r.y)+1.,h));
            let picture=t.picture_area(w,h);assert!(picture.height>0&&picture.y+picture.height==r.y);
            assert!(!t.chrome(f64::from(picture.y),h));
            assert_eq!(t.hit(1.,f64::from(t.height)),None);
        }
    }
    #[test]fn unavailable_actions_and_invalid_hits_are_inert(){
        let t=toolbar(800,400,1,false,false);assert!(t.actions[3].is_none()&&t.actions[4].is_none());
        assert_eq!(t.hit(f64::NAN,1.),None);assert_eq!(t.hit(-1.,1.),None);
        let t=toolbar(0,0,1,true,true);assert_eq!(t.height,0);
    }
    #[test]fn nonoverlapping_actions_and_footer_at_realistic_window_sizes(){
        for scale in 1..=3{for w in [450,640,1024,1920]{for h in [180,360,720]{
            let(width,height)=(w*scale,h*scale);
            let t=toolbar(width,height,scale,true,true);
            let p=t.picture_area(width,height);
            assert!(p.height>0);
            for (i,r) in t.actions.iter().enumerate(){
                let r=r.unwrap();assert!(r.x+r.width<=width&&r.y+r.height<=p.y);
                assert_eq!(t.hit(f64::from(r.x)+0.5,f64::from(r.y)+0.5),Some(i as u32));
                for (j,b) in t.actions.iter().enumerate(){if i!=j{let b=b.unwrap();assert!(r.x+r.width<=b.x||b.x+b.width<=r.x||r.y+r.height<=b.y||b.y+b.height<=r.y);}}
            }
            let f=t.summary.unwrap();assert_eq!(p.y+p.height,f.y);
            assert!(t.chrome(f64::from(f.y),height));
            assert!(crate::viewport::map_pointer((f64::from(width)/2.,f64::from(f.y)),crate::viewport::fit((960,540),p).unwrap(),(960,540),Rect{x:0,y:0,width:960,height:540}).is_none());
        }}}
        let t=toolbar(1024,720,1,true,true);
        assert!(t.actions[0].unwrap().x>t.actions[1].unwrap().x);
        assert_eq!(t.hit(0.,0.),None);
    }
    #[test]fn locales_are_closed_and_bitmap_safe(){
        assert_eq!(Locale::Zh.localized_labels()[0],"断开连接");assert_eq!(Locale::Ja.localized_labels()[2],"閲覧のみ");assert_eq!(Locale::Ko.localized_labels()[0],"연결 해제");
        for i in 0..8{let l=Locale::from_index(i);assert_eq!(Locale::parse(l.code()),Some(l));assert!(l.labels().iter().all(|s|s.is_ascii()&&s.len()<=15));}
        assert!(Locale::parse("remote-string").is_none());
    }
}
