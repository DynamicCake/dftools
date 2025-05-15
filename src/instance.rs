use ascii_domain::{
    char_set::ASCII_HYPHEN_DIGITS_LOWERCASE,
    dom::{Domain, DomainErr},
};
use serde::{Deserialize, Serialize};

/// Represents an instance
// If none, the instance is refering to itself
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Instance(Option<Domain<String>>);

impl TryFrom<String> for Instance {
    type Error = DomainErr;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::convert(value)
    }
}

impl TryFrom<Option<String>> for Instance {
    type Error = DomainErr;

    fn try_from(value: Option<String>) -> Result<Self, Self::Error> {
        let domain: Option<Domain<String>> = match value {
            Some(it) => Some(Domain::try_from_bytes(it, &ASCII_HYPHEN_DIGITS_LOWERCASE)?),
            None => None,
        };
        Ok(Instance(domain))
    }
}

impl From<Instance> for Option<String> {
    fn from(val: Instance) -> Self {
        val.0.map(|it| it.into_inner())
    }
}

impl<'a> From<&'a Instance> for Option<&'a String> {
    fn from(val: &'a Instance) -> Self {
        if let Some(val) = &val.0 {
            Some(val.as_inner())
        } else {
            None
        }
    }
}

impl Instance {
    pub fn inner(&self) -> &Option<Domain<String>> {
        &self.0
    }
    pub async fn vibe_check(&self) -> bool {
        // TODO: Implment vibe check
        true
    }
    fn convert(str: String) -> Result<Self, DomainErr> {
        if str.is_empty() {
            return Ok(Instance(None))
        }
        let domain = Domain::try_from_bytes(str, &ASCII_HYPHEN_DIGITS_LOWERCASE)?;
        Ok(Instance(Some(domain)))
    }
}
