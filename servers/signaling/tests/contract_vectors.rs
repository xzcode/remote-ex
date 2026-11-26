//! Fixed synthetic public vectors only. No fixture private key is present,
//! no real signaling, device enrollment, media session, or OS input.
use remote_ex_security::{self as security,Role,SessionTranscript,route_binding::RouteBinding};
use remote_ex_signaling::{decode_hex,hex};
use serde_json::Value;
fn fixture()->Value{serde_json::from_str(include_str!("../../../docs/protocol/v1-fixture.json")).unwrap()}
fn bytes<const N:usize>(v:&Value,name:&str)->[u8;N]{decode_hex::<N>(v[name].as_str().unwrap()).unwrap()}
#[test]
fn independent_v1_public_vectors(){
    let v=fixture();assert_eq!(v["fixture_only"],true);
    let c=bytes::<32>(&v,"controller_key");let h=bytes::<32>(&v,"host_key");
    let server=bytes::<32>(&v,"server_id");let challenge=bytes::<32>(&v,"challenge");
    let reg=bytes::<64>(&v,"registration_signature");let ctl=bytes::<64>(&v,"controller_registration_signature");
    assert_eq!(hex(&security::fingerprint(&c)),v["controller_id"].as_str().unwrap());
    assert!(security::verify_registration(&c,&server,&challenge,&reg));
    assert!(security::verify_controller_registration(&c,&server,&challenge,&ctl));
    assert!(!security::verify_controller_registration(&c,&server,&challenge,&reg));
    assert!(!security::verify_registration(&c,&server,&challenge,&ctl));
    let b=RouteBinding{session:bytes(&v,"session_id"),controller:c,host:h,controller_nonce:bytes(&v,"controller_nonce"),host_nonce:bytes(&v,"host_nonce"),requested:3,granted:3,profile:0,relay_only:false};
    assert_eq!(hex(&b.context().unwrap()),v["route_context"].as_str().unwrap());
    let hash=bytes::<32>(&v,"noise_hash");let cs=bytes::<64>(&v,"controller_route_signature");
    assert!(b.verify(Role::Controller,&hash,&cs).is_ok());
    assert!(b.verify(Role::Host,&hash,&bytes(&v,"host_route_signature")).is_ok());
    let mut changed=b.clone();changed.relay_only=true;assert!(changed.verify(Role::Controller,&hash,&cs).is_err());
    let mut changed=b.clone();changed.profile=2;assert!(changed.verify(Role::Controller,&hash,&cs).is_err());
    let offer=v["offer"].as_str().unwrap().as_bytes();let answer=v["answer"].as_str().unwrap().as_bytes();
    let t=SessionTranscript{session_id:b.session,controller_key:c,host_key:h,controller_nonce:b.controller_nonce,host_nonce:b.host_nonce,requested:3,granted:3,offer_sha256:security::hash_sdp(offer).unwrap(),answer_sha256:security::hash_sdp(answer).unwrap()};
    let a=bytes::<64>(&v,"controller_sdp_signature");let z=bytes::<64>(&v,"host_sdp_signature");
    assert!(security::verify_session(t.clone(),&c,&h,offer,answer,&a,&z).is_ok());
    assert!(security::verify_session(t,&c,&h,b"changed offer",answer,&a,&z).is_err());
    let input=remote_ex_protocol::decode_input(&bytes::<24>(&v,"release_all_protobuf")).unwrap();
    assert_eq!(input.session,b.session);assert_eq!(input.sequence,1);assert_eq!(input.input,remote_ex_protocol::Input::ReleaseAll);
}
