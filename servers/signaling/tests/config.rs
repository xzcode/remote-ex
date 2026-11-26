//! Offline checks use a generated local fixture, never a deployment secret.
use std::{process::{Command,Output},path::Path};
const BIN:&str=env!("CARGO_BIN_EXE_remote-ex-rendezvous");
fn check(vars:&[(&str,&str)])->Output{
    let mut c=Command::new(BIN);
    for (key,_) in std::env::vars_os(){if key.to_string_lossy().starts_with("REMOTE_EX_"){c.env_remove(key);}}
    c.arg("--check-config").envs(vars.iter().copied()).output().unwrap()
}
fn report(out:Output)->serde_json::Value{assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));serde_json::from_slice(&out.stdout).unwrap()}
#[test]
fn preflight_never_binds_and_rejects_insecure_configuration(){
    let held=std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address=held.local_addr().unwrap().to_string();
    let r=report(check(&[("REMOTE_EX_SIGNAL_BIND",&address)]));
    assert_eq!(r["network_started"],false);assert_eq!(r["mode"],"loopback_ws");
    for vars in [
        vec![("REMOTE_EX_SIGNAL_BIND","0.0.0.0:47822")],
        vec![("REMOTE_EX_SIGNAL_BIND","0.0.0.0:47822"),("REMOTE_EX_PUBLIC_BIND","1")],
        vec![("REMOTE_EX_PUBLIC_BIND","yes")],
        vec![("REMOTE_EX_TLS_CERT","")],
        vec![("REMOTE_EX_TLS_CERT","missing.pem")],
        vec![("REMOTE_EX_STUN_URL","stun://localhost:3478")],
        vec![("REMOTE_EX_RELAY_POOL_FILE","missing.json")],
        vec![("REMOTE_EX_RELAY_REGION","asia")],
        vec![("REMOTE_EX_WEB_ORIGIN","https://remote.example.net")],
        vec![("REMOTE_EX_WEB_ORIGIN","*")],
        vec![("REMOTE_EX_WEB_ORIGINS","https://one.example.net,https://two.example.net")],
    ]{assert!(!check(&vars).status.success());}
}
#[test]
fn enrolled_tls_checks_cert_allowlist_and_turn_without_printing_secrets(){
    let dir=tempfile::tempdir().unwrap();
    let rcgen::CertifiedKey{cert,key_pair}=rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    fn write(d:&Path,n:&str,data:&str)->String{let p=d.join(n);std::fs::write(&p,data).unwrap();p.to_str().unwrap().to_owned()}
    let cert=write(dir.path(),"cert.pem",&cert.pem());let key=write(dir.path(),"key.pem",&key_pair.serialize_pem());
    let id="ab".repeat(32);let allow=write(dir.path(),"allow.txt",&format!("{}\n", "cd".repeat(32)));
    let secret_text="this-is-a-test-only-secret-not-for-deployment-123";
    let secret=write(dir.path(),"turn.secret",secret_text);
    let vars=[("REMOTE_EX_SIGNAL_BIND","0.0.0.0:47822"),("REMOTE_EX_PUBLIC_BIND","1"),("REMOTE_EX_TLS_CERT",cert.as_str()),("REMOTE_EX_TLS_KEY",key.as_str()),("REMOTE_EX_SERVER_ID",id.as_str()),("REMOTE_EX_ALLOWED_DEVICES_FILE",allow.as_str()),("REMOTE_EX_STUN_URL","stun://relay.example.net:3478"),("REMOTE_EX_TURN_URL","turns://relay.example.net:5349"),("REMOTE_EX_TURN_SECRET_FILE",secret.as_str())];
    let out=check(&vars);assert!(!String::from_utf8_lossy(&out.stdout).contains(secret_text));assert!(!String::from_utf8_lossy(&out.stdout).contains(&id));
    let r=report(out);assert_eq!(r["mode"],"wss");assert_eq!(r["enrolled_count"],1);assert_eq!(r["turn_configured"],true);assert_eq!(r["relay_count"],1);assert_eq!(r["eligible_relay_count"],1);
    let mut multi=vars.to_vec();multi.push(("REMOTE_EX_WEB_ORIGINS","https://appassets.androidplatform.net,https://mobile.example.net"));
    let r=report(check(&multi));assert_eq!(r["web_origin_count"],2);assert_eq!(r["web_origin_configured"],true);
    assert!(!r.to_string().contains("mobile.example.net"));
    multi.push(("REMOTE_EX_WEB_ORIGIN","https://mobile.example.net"));assert!(!check(&multi).status.success());
    let mut invalid=vars.to_vec();invalid.push(("REMOTE_EX_WEB_ORIGINS","https://mobile.example.net,https://mobile.example.net"));assert!(!check(&invalid).status.success());
    std::fs::write(&allow,"# empty").unwrap();assert!(!check(&vars).status.success());
    std::fs::write(&allow,"cd".repeat(32)).unwrap();std::fs::write(&secret,"short").unwrap();assert!(!check(&vars).status.success());
}

#[test]
fn offline_admin_cli_stages_verified_ids_without_starting_network(){
    let d=tempfile::tempdir().unwrap();
    let original=d.path().join("active.txt");let staged=d.path().join("staged.txt");let revoked=d.path().join("revoked.txt");
    let a=remote_ex_security::DeviceIdentity::generate();let b=remote_ex_security::DeviceIdentity::generate();
    let aid=remote_ex_signaling::hex(&a.fingerprint());let bid=remote_ex_signaling::hex(&b.fingerprint());
    let ak=remote_ex_signaling::hex(&a.public_key());let bk=remote_ex_signaling::hex(&b.public_key());
    let x=original.to_str().unwrap();let y=staged.to_str().unwrap();let z=revoked.to_str().unwrap();
    let run=|args:&[&str]|Command::new(BIN).args(args).output().unwrap();
    let r=report(run(&["--new-allowlist",x,&aid,&ak]));assert_eq!(r["active_config_changed"],false);
    assert!(!run(&["--enroll-plan",x,x,&bid,&bk]).status.success());
    assert!(!run(&["--enroll-plan",x,y,&bid,&ak]).status.success());
    let r=report(run(&["--enroll-plan",x,y,&bid,&bk]));assert_eq!(r["after"]["count"],2);
    let diff=report(run(&["--allowlist-diff",x,y]));assert_eq!(diff["added"],1);assert_eq!(diff["removed"],0);
    let r=report(run(&["--revoke-plan",y,z,&aid]));assert_eq!(r["after"]["count"],1);
    assert!(!run(&["--revoke-plan",z,d.path().join("empty.txt").to_str().unwrap(),&bid]).status.success());
    assert_eq!(report(run(&["--allowlist-info",x]))["count"],1);
    let matrix=report(run(&["--protocol-info"]));assert_eq!(matrix["profiles"].as_array().unwrap().len(),28);assert_eq!(matrix["network_started"],false);
}
