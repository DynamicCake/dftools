use ascii_domain::{
    char_set::AllowedAscii,
    dom::{Domain, DomainErr},
};
use base64::Engine;
use color_eyre::eyre::Context;
use ed25519_dalek::VerifyingKey;
use poem_openapi::Object;
use serde::{Deserialize, Serialize};

use crate::BASE64;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Instance {
    pub key: VerifyingKey,
    pub domain: InstanceDomain,
}

/// Gets converted into an ExternalInstance
#[derive(Debug, Serialize, Deserialize, Clone, Object)]
pub struct SendInstance {
    /// Base64 encoded
    pub key: String,
    pub domain: String,
}

impl SendInstance {
    pub fn parse(&self) -> color_eyre::Result<Instance> {
        let decoded = BASE64.decode(&self.key)?;
        Ok(Instance {
            key: VerifyingKey::from_bytes(
                decoded
                    .as_slice()
                    .try_into()
                    .wrap_err("Error converting to [u8; 32]")?,
            )?,
            domain: InstanceDomain::External(ExternalDomain::convert(self.domain.clone())?),
        })
    }
}

impl Instance {
    pub fn new(key: VerifyingKey, domain: InstanceDomain) -> Self {
        Self { key, domain }
    }
    pub fn from_row(public_key: Vec<u8>, domain: Option<String>) -> color_eyre::Result<Self> {
        Ok(Instance::new(
            VerifyingKey::from_bytes(public_key.as_slice().try_into()?)?,
            InstanceDomain::from_option(domain)?,
        ))
    }
    pub fn encode(&self, this_instance: &str) -> String {
        let domain = &self.domain;
        let domain = match domain {
            InstanceDomain::External(ext) => ext.inner(),
            InstanceDomain::Current => this_instance,
        };
        format!("{};{}", domain, BASE64.encode(self.key))
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum InstanceDomain {
    External(ExternalDomain),
    Current,
}

impl InstanceDomain {
    pub fn from_option(domain: Option<String>) -> Result<InstanceDomain, DomainErr> {
        Ok(match domain {
            Some(ext) => InstanceDomain::External(ext.try_into()?),
            None => InstanceDomain::Current,
        })
    }
}

/// Represents an instance domain
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ExternalDomain(Domain<String>);

impl TryFrom<String> for ExternalDomain {
    type Error = DomainErr;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::convert(value)
    }
}

impl ExternalDomain {
    pub fn inner(&self) -> &Domain<String> {
        &self.0
    }
    pub fn into_inner(self) -> Domain<String> {
        self.0
    }
    fn convert(str: String) -> Result<Self, DomainErr> {
        let allowed: AllowedAscii<[u8; 38]> = AllowedAscii::try_from_unique_ascii([
            b'-', b'0', b'1', b'2', b'3', b'4', b'5', b'6', b'7', b'8', b'9', b'a', b'b', b'c',
            b'd', b'e', b'f', b'g', b'h', b'i', b'j', b'k', b'l', b'm', b'n', b'o', b'p', b'q',
            b'r', b's', b't', b'u', b'v', b'w', b'x', b'y', b'z', b':',
        ])
        .expect("fit all criteria");
        let domain = Domain::try_from_bytes(str, &allowed)?;
        Ok(ExternalDomain(domain))
    }
}
