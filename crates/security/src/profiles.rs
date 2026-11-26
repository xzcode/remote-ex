//! Stable v1 profile numbers in the signed route transcript. Descriptions
//! are not a platform support probe or user approval. Never repurpose a code.
use crate::session::{VIEW,INPUT};
pub const MAX_PROFILE:u8=27;
pub const NOISE_PATTERN:&str="Noise_NN_25519_ChaChaPoly_BLAKE2s";
pub const MAX_SEALED_CLEAR:usize=32768;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct Profile{
    pub code:u8,pub name:&'static str,pub grant:u8,pub real_screen:bool,
    pub real_input:bool,pub system_audio:bool,pub fixture:bool,pub extended:bool,
}
const fn p(code:u8,name:&'static str,grant:u8,screen:bool,input:bool,audio:bool,fixture:bool,extended:bool)->Profile{
    Profile{code,name,grant,real_screen:screen,real_input:input,system_audio:audio,fixture,extended}
}
const V:u8=VIEW;const C:u8=VIEW|INPUT;
pub const ALL:[Profile;28]=[
    p(0,"synthetic_test",C,false,false,false,true,false),
    p(1,"screen_view",V,true,false,false,false,false),
    p(2,"screen_control",C,true,true,false,false,false),
    p(3,"file_controller_sends",V,false,false,false,false,false),
    p(4,"file_host_sends",V,false,false,false,false,false),
    p(5,"text_controller_sends",V,false,false,false,false,false),
    p(6,"text_host_sends",V,false,false,false,false,false),
    p(7,"png_controller_sends",V,false,false,false,false,false),
    p(8,"png_host_sends",V,false,false,false,false,false),
    p(9,"synthetic_audio",V,false,false,false,true,false),
    p(10,"system_audio",V,false,false,true,false,false),
    p(11,"directory_controller_sends",V,false,false,false,false,false),
    p(12,"directory_host_sends",V,false,false,false,false,false),
    p(13,"synthetic_clipboard_sync",V,false,false,false,true,false),
    p(14,"live_text_clipboard_sync",V,false,false,false,false,false),
    p(15,"read_only_catalog",V,false,false,false,false,false),
    p(16,"synthetic_av",C,false,false,false,true,false),
    p(17,"screen_audio_view",V,true,false,true,false,false),
    p(18,"screen_audio_control",C,true,true,true,false,false),
    p(19,"reviewed_file_changes",V,false,false,false,false,false),
    p(20,"read_only_catalog_batch",V,false,false,false,false,false),
    p(21,"extended_synthetic_test",C,false,false,false,true,true),
    p(22,"extended_screen_view",V,true,false,false,false,true),
    p(23,"extended_screen_control",C,true,true,false,false,true),
    p(24,"extended_screen_audio_view",V,true,false,true,false,true),
    p(25,"extended_screen_audio_control",C,true,true,true,false,true),
    p(26,"inert_power_fixture",V,false,false,false,true,false),
    p(27,"locally_reviewed_power",V,false,false,false,false,false),
];
pub fn get(code:u8)->Option<&'static Profile>{ALL.get(usize::from(code))}
/// Preserve v1 verifier behavior: profile 0 accepts VIEW or VIEW|INPUT,
/// while the routed owner uses grant=3 for that synthetic-only fixture.
pub fn binding_grant_valid(code:u8,grant:u8)->bool{
    get(code).is_some_and(|p|grant==p.grant||(code==0&&grant==VIEW))
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn exact_codes_and_legacy_masks(){
        assert_eq!(ALL.len(),usize::from(MAX_PROFILE)+1);
        for (i,p) in ALL.iter().enumerate(){
            assert_eq!(usize::from(p.code),i);
            assert_eq!(p.grant,if matches!(i,0|2|16|18|21|23|25){3}else{1});
            for grant in 0..=255u8{assert_eq!(binding_grant_valid(p.code,grant),grant==p.grant||(i==0&&grant==1));}
            if p.fixture{assert!(!p.real_screen&&!p.real_input&&!p.system_audio);}
            if p.real_input{assert!(p.real_screen&&p.grant==3);}
            assert_eq!(p.extended,(21..=25).contains(&i));
        }
        assert!(get(28).is_none());assert!(!binding_grant_valid(255,3));
    }
}
