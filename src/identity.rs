use crate::ResourceId;

use rsa::{RsaPrivateKey, RsaPublicKey, Pkcs1v15Encrypt};
use rand::prelude::*;
use serde::{Serialize, Deserialize};
use std::fmt::{self, Write as _};

#[derive(Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicId(RsaPublicKey);

impl PublicId {
    pub fn resource_id(&self) -> ResourceId {
        struct Linear(u64);

        impl rand::RngCore for Linear {
            fn next_u32(&mut self) -> u32 { self.0 += 1; self.0 as u32 }
            fn next_u64(&mut self) -> u64 { self.0 += 1; self.0 }
            fn fill_bytes(&mut self, dest: &mut [u8]) { dest.fill_with(|| self.next_u64() as u8); }
            fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand::Error> { Ok(self.fill_bytes(dest)) }
        }

        impl rand::CryptoRng for Linear {}

        ResourceId(<[_; 32]>::try_from(&self.0.encrypt(&mut Linear(0), Pkcs1v15Encrypt, &[0; 32]).unwrap()[0..32]).unwrap())
    }

    pub fn human_readable_name(&self, entropy: usize) -> String {
        let mut name = String::new();
        for b in self.resource_id().0.into_iter().take(entropy) {
            write!(
                name,
                "{}{}",
                if name.is_empty() { "" } else { "_" },
                include_str!("../data/words.txt").lines().nth(b as usize).unwrap().trim(),
            ).unwrap();
        }
        name
    }
}

impl fmt::Debug for PublicId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.human_readable_name(2))
        // write!(f, "{:<032X}", self.resource_id())
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
