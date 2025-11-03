use std::fmt::Display;

use base64::{Engine, prelude::BASE64_STANDARD};

use crate::error::Error;

#[derive(serde::Deserialize, Debug, Clone, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "lowercase")]
pub enum EmailTemplateTypeLegacy {
    #[default]
    Default,
    Metaspexet,
    None,
}

impl Display for EmailTemplateTypeLegacy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EmailTemplateTypeLegacy::Default => write!(f, "default"),
            EmailTemplateTypeLegacy::Metaspexet => write!(f, "metaspexet"),
            EmailTemplateTypeLegacy::None => write!(f, "none"),
        }
    }
}

impl From<EmailTemplateTypeLegacy> for String {
    fn from(template: EmailTemplateTypeLegacy) -> Self {
        match template {
            EmailTemplateTypeLegacy::Default => "default".to_string(),
            EmailTemplateTypeLegacy::Metaspexet => "metaspexet".to_string(),
            EmailTemplateTypeLegacy::None => "none".to_string(),
        }
    }
}

#[derive(serde::Deserialize, Debug, Clone)]
pub struct EmailNameLegacy {
    pub name: String,
    pub address: String,
}

#[derive(serde::Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum AddressFieldLegacy {
    Address(String),
    NameAndAddress(EmailNameLegacy),
}

impl TryFrom<AddressFieldLegacy> for String {
    type Error = Error;

    fn try_from(value: AddressFieldLegacy) -> Result<Self, Self::Error> {
        match value {
            AddressFieldLegacy::Address(addr) => match addr.is_ascii() {
                true => Ok(addr.clone()),
                _ => Err(Error::NotASCII("address field".to_string())),
            },
            AddressFieldLegacy::NameAndAddress(name_addr) => {
                let name = if name_addr.name.is_ascii() {
                    &name_addr.name
                } else {
                    &format!("=?UTF-8?B?{}?=", BASE64_STANDARD.encode(&name_addr.name))
                };
                if !name_addr.address.is_ascii() {
                    return Err(Error::NotASCII("address field".to_string()));
                }
                Ok(format!("{} <{}>", name, name_addr.address))
            }
        }
    }
}

impl TryFrom<&AddressFieldLegacy> for String {
    type Error = Error;

    fn try_from(value: &AddressFieldLegacy) -> Result<Self, Self::Error> {
        match value {
            AddressFieldLegacy::Address(addr) => match addr.is_ascii() {
                true => Ok(addr.to_owned()),
                _ => Err(Error::NotASCII("address field".to_string())),
            },
            AddressFieldLegacy::NameAndAddress(name_addr) => {
                let name = match name_addr.name.is_ascii() {
                    true => &name_addr.name,
                    _ => &format!("=?UTF-8?B?{}?=", BASE64_STANDARD.encode(&name_addr.name)),
                };
                if !name_addr.address.is_ascii() {
                    return Err(Error::NotASCII("address field".to_string()));
                }
                Ok(format!("{} <{}>", name, name_addr.address))
            }
        }
    }
}

fn encoding_default() -> String {
    "base64".to_string()
}

#[derive(serde::Deserialize, Debug, Clone)]
pub struct AttachmentLegacy {
    #[serde(rename = "originalname")]
    pub original_name: String,
    pub mimetype: String,
    pub buffer: String,
    #[serde(default = "encoding_default")]
    pub encoding: String,
}

#[derive(serde::Deserialize, Debug, Clone)]
pub struct EmailRequestLegacy {
    pub key: String,
    #[serde(default)]
    pub template: EmailTemplateTypeLegacy,
    pub from: AddressFieldLegacy,
    #[serde(rename = "replyTo")]
    pub reply_to: Option<AddressFieldLegacy>,
    pub to: Option<Vec<AddressFieldLegacy>>,
    pub subject: String,
    pub content: Option<String>,
    pub html: Option<String>,
    pub cc: Option<Vec<AddressFieldLegacy>>,
    pub bcc: Option<Vec<AddressFieldLegacy>>,
    #[serde(rename = "attachments[]")]
    pub attachments: Option<Vec<AttachmentLegacy>>,
}
