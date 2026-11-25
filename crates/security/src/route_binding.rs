//! Authentication of an ephemeral Noise handshake by the independently
//! verified device signing keys. No authorization is granted by this proof.
use super::{DeviceIdentity, PublicKey, Role, Signature, Signer, VerifyingKey};
use crate::session::{VIEW,INPUT,ProofError};

#[derive(Clone)]
pub struct RouteBinding {
    pub session: [u8;16],
    pub controller: PublicKey,
    pub host: PublicKey,
    pub controller_nonce: [u8;32],
    pub host_nonce: [u8;32],
    pub requested: u8,
    pub granted: u8,
    /// 0 = synthetic, 1 = real view, 2 = real control.
    pub profile:u8,
    pub relay_only:bool,
}
impl RouteBinding {
    pub fn context(&self)->Result<Vec<u8>,ProofError>{
        if self.session==[0;16]||self.controller==[0;32]||self.host==[0;32]
            ||self.controller==self.host||self.controller_nonce==[0;32]||self.host_nonce==[0;32]
            ||self.controller_nonce==self.host_nonce||self.requested&VIEW==0||self.granted&VIEW==0
            ||self.requested&!(VIEW|INPUT)!=0||self.granted&!self.requested!=0
            ||!matches!(self.profile,0..=14)||((self.profile==1||self.profile>=3)&&self.granted!=VIEW)||(self.profile==2&&self.granted!=(VIEW|INPUT)){
            return Err(ProofError::InvalidTranscript);
        }
        let mut b=b"Remote EX/routed-noise/v1\0".to_vec();
        b.extend_from_slice(&self.session);b.extend_from_slice(&self.controller);
        b.extend_from_slice(&self.host);b.extend_from_slice(&self.controller_nonce);
        b.extend_from_slice(&self.host_nonce);b.push(self.requested);b.push(self.granted);b.push(self.profile);b.push(u8::from(self.relay_only));Ok(b)
    }
    pub fn key(&self,role:Role)->PublicKey{match role{Role::Controller=>self.controller,Role::Host=>self.host}}
    fn bytes(&self,role:Role,hash:&[u8;32])->Result<Vec<u8>,ProofError>{
        if *hash==[0;32]{return Err(ProofError::InvalidTranscript);}
        let mut b=self.context()?;b.push(role as u8);b.extend_from_slice(hash);Ok(b)
    }
    pub fn sign(&self,id:&DeviceIdentity,role:Role,hash:&[u8;32])->Result<[u8;64],ProofError>{
        if id.public_key()!=self.key(role){return Err(ProofError::WrongIdentity);}
        Ok(id.0.sign(&self.bytes(role,hash)?).to_bytes())
    }
    pub fn verify(&self,role:Role,hash:&[u8;32],signature:&[u8;64])->Result<(),ProofError>{
        let key=VerifyingKey::from_bytes(&self.key(role)).map_err(|_|ProofError::WrongIdentity)?;
        key.verify_strict(&self.bytes(role,hash)?,&Signature::from_bytes(signature))
            .map_err(|_|ProofError::InvalidSignature)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn proof_binds_handshake_role_permission_and_both_nonces(){
        let c=DeviceIdentity::generate();let h=DeviceIdentity::generate();
        let b=RouteBinding{session:[1;16],controller:c.public_key(),host:h.public_key(),
            controller_nonce:[2;32],host_nonce:[3;32],requested:VIEW|INPUT,granted:VIEW,profile:0,relay_only:false};
        let s=b.sign(&c,Role::Controller,&[4;32]).unwrap();
        assert!(b.verify(Role::Controller,&[4;32],&s).is_ok());
        assert!(b.verify(Role::Controller,&[5;32],&s).is_err());
        assert!(b.verify(Role::Host,&[4;32],&s).is_err());
        let mut changed=b.clone();changed.granted=VIEW|INPUT;
        assert!(changed.verify(Role::Controller,&[4;32],&s).is_err());
    }
}
