//! Offline implementation description, not a permission or negotiation.
use remote_ex_security::profiles;
use serde_json::{json,Value};
pub fn info()->Value{
    let rows:Vec<_>=profiles::ALL.iter().map(|p|json!({
        "code":p.code,"name":p.name,"session_owner_grant":p.grant,
        "binding_accepts":if p.code==0{vec![1,3]}else{vec![p.grant]},
        "real_screen":p.real_screen,"real_input":p.real_input,"system_audio":p.system_audio,
        "fixture_only":p.fixture,"extended":p.extended,
        "web_mobile_implemented":false,
    })).collect();
    json!({"schema":1,"signaling_version":crate::protocol::VERSION,
      "input_version":remote_ex_protocol::VERSION,"max_message_bytes":crate::protocol::MAX_MESSAGE,
      "max_sdp_bytes":crate::protocol::MAX_SDP,"max_ice_candidate_bytes":crate::protocol::MAX_ICE,
      "max_input_bytes":remote_ex_protocol::MAX_INPUT_BYTES,"extended_route_seconds":crate::protocol::EXTENDED_ROUTE_SECONDS,
      "noise_pattern":profiles::NOISE_PATTERN,"max_sealed_clear_bytes":profiles::MAX_SEALED_CLEAR,
      "profiles":rows,"requires_verified_peer_keys":true,"requires_local_consent":true,"network_started":false})
}
#[cfg(test)]mod tests{
    #[test]fn contract_is_complete_and_not_runtime_authority(){
        let c=super::info();let rows=c["profiles"].as_array().unwrap();
        assert_eq!(rows.len(),28);assert_eq!(c["network_started"],false);
        for (i,row) in rows.iter().enumerate(){assert_eq!(row["code"],i);assert_eq!(row["web_mobile_implemented"],false);}
    }
}
