use rsa::{RsaPrivateKey, RsaPublicKey, Pkcs1v15Encrypt};
use rand::prelude::*;
use serde::{Serialize, Deserialize};
use std::fmt;

#[derive(Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicId(RsaPublicKey);

impl fmt::Debug for PublicId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        struct Linear(u64);

        impl rand::RngCore for Linear {
            fn next_u32(&mut self) -> u32 { self.0 += 1; self.0 as u32 }
            fn next_u64(&mut self) -> u64 { self.0 += 1; self.0 }
            fn fill_bytes(&mut self, dest: &mut [u8]) { dest.fill_with(|| self.next_u64() as u8); }
            fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand::Error> { Ok(self.fill_bytes(dest)) }
        }

        impl rand::CryptoRng for Linear {}

        write!(f, "{:<08X}", u32::from_le_bytes(<[_; 4]>::try_from(&self.0.encrypt(&mut Linear(0), Pkcs1v15Encrypt, &[0; 4]).unwrap()[0..4]).unwrap()))
    }
}

pub struct PrivateId(RsaPrivateKey);

impl PrivateId {
    pub fn generate() -> Self {
        Self(RsaPrivateKey::new(&mut thread_rng(), 2048).unwrap())
    }

    pub fn to_public(&self) -> PublicId {
        PublicId(self.0.to_public_key())
    }
}

impl fmt::Debug for PrivateId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.to_public().fmt(f)
    }
}
