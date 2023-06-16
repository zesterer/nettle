use rand::prelude::*;
use rsa::{traits::PublicKeyParts, RsaPublicKey};
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String")]
#[serde(into = "String")]
pub struct Tag([u8; 32]);

impl std::ops::Deref for Tag {
    type Target = [u8; 32];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl TryFrom<String> for Tag {
    type Error = &'static str;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::try_from_hex(s)
    }
}

impl Into<String> for Tag {
    fn into(self) -> String {
        hex::encode(self.0)
    }
}

impl Tag {
    pub fn try_from_hex<B: AsRef<[u8]>>(hex: B) -> Result<Self, &'static str> {
        let mut bytes = [0; 32];
        hex::decode_to_slice(hex, &mut bytes).map_err(|_| "malformed tag")?;
        Ok(Self::from_bytes(bytes))
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn digest_many<B: AsRef<[u8]>, I: IntoIterator<Item = B>>(bytes: I) -> Self {
        use sha3::{Digest, Sha3_256};
        let mut hasher = Sha3_256::new();
        for bytes in bytes {
            hasher.update(bytes);
        }
        Self(hasher.finalize().into())
    }

    pub fn fingerprint(key: &RsaPublicKey) -> Self {
        Self::digest_many([key.n(), key.e()].map(|x| x.to_bytes_le()))
    }

    pub fn digest<B: AsRef<[u8]>>(bytes: B) -> Self {
        Self::digest_many([bytes])
    }

    pub fn generate() -> Self {
        Self(thread_rng().gen())
    }

    pub fn dist_to(&self, other: Self) -> Self {
        let mut dist = self.0;
        for i in 0..dist.len() {
            dist[i] ^= other.0[i];
        }
        Self(dist)
    }

    // log2, effectively
    pub fn level(&self) -> u16 {
        self.0
            .into_iter()
            .enumerate()
            .find_map(|(i, b)| {
                if b != 0 {
                    Some(255 - i as u16 * 8 - b.ilog2() as u16)
                } else {
                    None
                }
            })
            .unwrap_or(0)
    }
}
