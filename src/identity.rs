use crate::Tag;

use rand::prelude::*;
use rand_chacha::ChaCha20Rng;
use rsa::{RsaPrivateKey, RsaPublicKey};
use serde::{Deserialize, Serialize};
use std::fmt::{self, Write as _};

#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(from = "RsaPublicKey")]
#[serde(into = "RsaPublicKey")]
pub struct PublicId {
    pub tag: Tag,
    pub key: RsaPublicKey,
}

impl PublicId {
    pub fn human_readable_name(&self, entropy: usize) -> String {
        let mut name = String::new();
        for b in self.tag.into_iter().take(entropy) {
            write!(
                name,
                "{}{}",
                if name.is_empty() { "" } else { "_" },
                include_str!("../data/words.txt")
                    .lines()
                    .nth(b as usize)
                    .unwrap()
                    .trim(),
            )
            .unwrap();
        }
        name
    }
}

impl From<RsaPublicKey> for PublicId {
    fn from(key: RsaPublicKey) -> Self {
        Self {
            tag: Tag::fingerprint(&key),
            key,
        }
    }
}

impl Into<RsaPublicKey> for PublicId {
    fn into(self) -> RsaPublicKey {
        self.key
    }
}

impl fmt::Debug for PublicId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.human_readable_name(2))
        // write!(f, "{:<032X}", self.resource_id())
    }
}

pub struct PrivateId {
    pub pub_id: PublicId,
    #[allow(dead_code)]
    priv_tag: Tag,
    #[allow(dead_code)]
    priv_key: RsaPrivateKey,
}

impl PrivateId {
    pub fn from_seed<B: AsRef<[u8]>>(bytes: B) -> Self {
        // The private tag should not be revealed, since it acts as the seed for deriving the key pair
        let priv_tag = Tag::digest(bytes);
        // Generate the key pair from the private tag in a deterministic manner
        let priv_key = RsaPrivateKey::new(&mut ChaCha20Rng::from_seed(*priv_tag), 2048).unwrap();

        Self {
            pub_id: PublicId::from(priv_key.to_public_key()),
            priv_tag,
            priv_key,
        }
    }

    pub fn generate() -> Self {
        Self::from_seed(&thread_rng().gen::<[u8; 32]>())
    }
}

impl fmt::Debug for PrivateId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.pub_id.fmt(f)
    }
}
