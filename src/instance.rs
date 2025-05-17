use ascii_domain::{
    char_set::ASCII_HYPHEN_DIGITS_LOWERCASE,
    dom::{Domain, DomainErr},
};
use base64::{prelude::BASE64_STANDARD, Engine};
use ed25519_dalek::VerifyingKey;
use jwt::ToBase64;
use poem_openapi::Object;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Instance {
    pub key: VerifyingKey,
    pub domain: InstanceDomain,
}

#[derive(Debug, Serialize, Deserialize, Clone, Object)]
pub struct SendInstance {
    /// Base64 encoded
    pub key: String,
    pub domain: String,
}

impl SendInstance {
    pub fn parse(&self) -> color_eyre::Result<Instance> {
        Ok(Instance {
            key: VerifyingKey::from_bytes(
                BASE64_STANDARD.decode(&self.key)?.as_slice().try_into()?,
            )?,
            domain: self.domain.clone().try_into()?,
        })
    }
}

impl Instance {
    pub fn new(key: VerifyingKey, domain: InstanceDomain) -> Self {
        Self { key, domain }
    }
    pub fn from_row(public_key: Vec<u8>, domain: String) -> color_eyre::Result<Self> {
        Ok(Instance::new(
            VerifyingKey::from_bytes(public_key.as_slice().try_into()?)?,
            domain.try_into()?,
        ))
    }
    pub fn encode(&self, this_instance: &str) -> String {
        let domain = &self.domain;
        let domain: Option<&str> = domain.into();
        let domain = domain.unwrap_or(this_instance);
        format!(
            "{}:{}",
            domain,
            self.key.to_base64().expect("Serde decided to fail")
        )
    }
}

/// Represents an instance domain, does not guarantee the instance exists
///
// If none, the instance is referring to itself
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InstanceDomain(Option<Domain<String>>);

impl TryFrom<String> for InstanceDomain {
    type Error = DomainErr;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::convert(value)
    }
}

impl TryFrom<Option<String>> for InstanceDomain {
    type Error = DomainErr;

    fn try_from(value: Option<String>) -> Result<Self, Self::Error> {
        let domain: Option<Domain<String>> = match value {
            Some(it) => Some(Domain::try_from_bytes(it, &ASCII_HYPHEN_DIGITS_LOWERCASE)?),
            None => None,
        };
        Ok(InstanceDomain(domain))
    }
}

impl From<InstanceDomain> for Option<String> {
    fn from(val: InstanceDomain) -> Self {
        val.0.map(|it| it.into_inner())
    }
}

impl<'a> From<&'a InstanceDomain> for Option<&'a str> {
    fn from(val: &'a InstanceDomain) -> Self {
        if let Some(val) = &val.0 {
            Some(val.as_inner())
        } else {
            None
        }
    }
}

impl InstanceDomain {
    pub fn inner(&self) -> &Option<Domain<String>> {
        &self.0
    }
    fn convert(str: String) -> Result<Self, DomainErr> {
        if str.is_empty() {
            return Ok(InstanceDomain(None));
        }
        let domain = Domain::try_from_bytes(str, &ASCII_HYPHEN_DIGITS_LOWERCASE)?;
        Ok(InstanceDomain(Some(domain)))
    }
}
