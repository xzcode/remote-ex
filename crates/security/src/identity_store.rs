//! Explicit persistent identity storage. Never replace an unreadable key,
//! silently fall back to a temporary identity or expose a seed to a webview.
use super::{DeviceIdentity, SigningKey};
use zeroize::Zeroizing;

/// Slot zero keeps the original M2 credential, so upgrades never silently
/// regenerate the established device ID. Extra slots are independent devices.
pub fn credential_name(slot:u8)->Result<&'static str,String>{
    match slot{0=>Ok("device-signing-v1"),1=>Ok("device-signing-slot2-v1"),2=>Ok("device-signing-slot3-v1"),_=>Err("Invalid device identity slot".into())}
}
pub trait SecretStore {
    fn load(&self) -> Result<Option<Zeroizing<Vec<u8>>>, String>;
    /// The caller must hold an exclusive creation lock. Never overwrite an
    /// existing credential. Re-read and verify after writing.
    fn create(&self, seed: &[u8;32]) -> Result<(), String>;
}

pub fn load_or_create(store: &impl SecretStore) -> Result<DeviceIdentity, String> {
    fn decode(seed: &[u8]) -> Result<DeviceIdentity, String> {
        let bytes: &[u8;32] = seed.try_into().map_err(|_| "Invalid stored identity length")?;
        if *bytes == [0;32] { return Err("Invalid stored identity".into()); }
        Ok(DeviceIdentity(SigningKey::from_bytes(bytes)))
    }
    if let Some(seed) = store.load()? { return decode(&seed); }
    let id = DeviceIdentity::generate();
    let seed = Zeroizing::new(id.0.to_bytes());
    store.create(&seed)?;
    let saved = store.load()?.ok_or("Identity not persisted")?;
    let confirmed = decode(&saved)?;
    if confirmed.public_key() != id.public_key() { return Err("Stored identity changed during creation".into()); }
    Ok(confirmed)
}

#[cfg(all(feature="native-vault", any(target_os="windows",target_os="macos")))]
pub mod native {
    use super::*;
    use fs2::FileExt;
    use std::fs::OpenOptions;
    struct Vault(keyring::Entry);
    impl SecretStore for Vault {
        fn load(&self) -> Result<Option<Zeroizing<Vec<u8>>>, String> {
            match self.0.get_secret() {
                Ok(seed) => Ok(Some(Zeroizing::new(seed))),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(_) => Err("Cannot read the system device identity credential".into()),
            }
        }
        fn create(&self, seed: &[u8;32]) -> Result<(), String> {
            if self.load()?.is_some() { return Err("Device identity already exists".into()); }
            self.0.set_secret(seed).map_err(|_| "Cannot save the system device identity credential".into())
        }
    }
    /// Called only after the user explicitly chooses persistent identity.
    /// File lock contains no secret and serializes cooperating installations.
    /// An inaccessible credential store is an error, never a fallback.
    pub fn open() -> Result<DeviceIdentity, String> {open_slot(0)}
    pub fn open_slot(slot:u8)->Result<DeviceIdentity,String>{
        let credential=credential_name(slot)?;
        let dir = dirs::data_local_dir().ok_or("No per-user data directory")?.join("Remote EX");
        std::fs::create_dir_all(&dir).map_err(|_| "Cannot create identity lock directory")?;
        let lock = OpenOptions::new().read(true).write(true).create(true).truncate(false)
            .open(dir.join("identity.lock")).map_err(|_| "Cannot open identity lock")?;
        lock.try_lock_exclusive().map_err(|_| "Another Remote EX instance is opening its identity")?;
        let vault = Vault(keyring::Entry::new("com.xzcode.remoteex", credential)
            .map_err(|_| "Cannot access the system credential store")?);
        let result = load_or_create(&vault);
        let _ = FileExt::unlock(&lock);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    #[derive(Default)]
    struct Memory(RefCell<Option<Vec<u8>>>);
    impl SecretStore for Memory {
        fn load(&self) -> Result<Option<Zeroizing<Vec<u8>>>,String>{
            Ok(self.0.borrow().clone().map(Zeroizing::new))
        }
        fn create(&self, seed:&[u8;32])->Result<(),String>{
            let mut s=self.0.borrow_mut();
            if s.is_some(){return Err("already exists".into());}
            *s=Some(seed.to_vec());Ok(())
        }
    }
    #[test]
    fn stable_credential_names_and_slot_bounds(){
        assert_eq!(credential_name(0).unwrap(),"device-signing-v1");
        assert_ne!(credential_name(0).unwrap(),credential_name(1).unwrap());
        assert_ne!(credential_name(1).unwrap(),credential_name(2).unwrap());
        assert!(credential_name(3).is_err());
    }
    #[test]
    fn same_identity_across_loads_and_corruption_fails_closed(){
        let store=Memory::default();
        let a=load_or_create(&store).unwrap();
        let b=load_or_create(&store).unwrap();
        assert_eq!(a.fingerprint(),b.fingerprint());
        *store.0.borrow_mut()=Some(vec![0;32]);
        assert!(load_or_create(&store).is_err());
        assert_eq!(store.0.borrow().as_deref(),Some([0u8;32].as_slice()));
    }
}
