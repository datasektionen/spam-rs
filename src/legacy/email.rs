use std::fmt::{Debug, Display};

use base64::{Engine, prelude::BASE64_STANDARD};
use serde::{Deserialize, Deserializer};
use serde_json::Value;

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
            EmailTemplateTypeLegacy::Default => "default",
            EmailTemplateTypeLegacy::Metaspexet => "metaspexet",
            EmailTemplateTypeLegacy::None => "none",
        }
        .to_string()
    }
}

#[derive(serde::Deserialize, Debug, Clone)]
pub struct EmailNameLegacy {
    pub name: String,
    pub address: String,
}

#[derive(Debug, Clone)]
pub enum AddressFieldLegacy {
    Address(String),
    NameAndAddress(EmailNameLegacy),
}

impl<'de> Deserialize<'de> for AddressFieldLegacy {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;

        match value {
            Value::String(s) => Ok(AddressFieldLegacy::Address(s)),
            Value::Object(obj) => {
                let name = obj
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| serde::de::Error::missing_field("name"))?
                    .to_string();
                let address = obj
                    .get("address")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| serde::de::Error::missing_field("address"))?
                    .to_string();
                Ok(AddressFieldLegacy::NameAndAddress(EmailNameLegacy {
                    name,
                    address,
                }))
            }
            _ => Err(serde::de::Error::custom("expected string or object")),
        }
    }
}

impl TryFrom<&AddressFieldLegacy> for String {
    type Error = Error;

    fn try_from(value: &AddressFieldLegacy) -> Result<Self, Self::Error> {
        match value {
            AddressFieldLegacy::Address(addr) => match addr.is_ascii() {
                true => Ok(addr.clone()),
                _ => {
                    // if address id form Name <addr>, then we just check that addr is ASCII, rest encoded as UTF-8
                    if addr.contains('<') {
                        let (name, addr) = addr
                            .split_once('<')
                            .ok_or(Error::InvalidAddress("".to_string()))?;

                        match addr.is_ascii() {
                            true => Ok(format!(
                                "{} <{}>",
                                format_utf8(name),
                                addr.trim_end_matches('>')
                            )),
                            _ => Err(Error::InvalidAddress("address is not ASCII".to_string())),
                        }
                    } else {
                        return Err(Error::InvalidAddress(
                            "is not ASCII and is not in Name <addr> format".to_string(),
                        ));
                    }
                }
            },
            AddressFieldLegacy::NameAndAddress(name_addr) => {
                let name = if name_addr.name.is_ascii() {
                    &name_addr.name
                } else {
                    &format_utf8(&name_addr.name)
                };
                if !name_addr.address.is_ascii() {
                    return Err(Error::NotASCII("address field".to_string()));
                }
                Ok(format!("{} <{}>", name, name_addr.address))
            }
        }
    }
}

fn format_utf8(name: &str) -> String {
    format!("=?UTF-8?B?{}?=", BASE64_STANDARD.encode(name.trim()))
}

impl TryFrom<AddressFieldLegacy> for String {
    type Error = Error;
    fn try_from(value: AddressFieldLegacy) -> Result<Self, Self::Error> {
        (&value).try_into()
    }
}

#[derive(Debug, Clone)]
pub enum AddressFieldsLegacy {
    AddressField(AddressFieldLegacy),
    AddressList(Vec<AddressFieldLegacy>),
}

impl<'de> Deserialize<'de> for AddressFieldsLegacy {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;

        match value {
            // String could be a single address OR comma-separated list
            Value::String(s) => {
                if s.contains(',') {
                    let addresses = s
                        .split(",")
                        .map(|s| AddressFieldLegacy::Address(s.trim().to_string()))
                        .collect();
                    Ok(Self::AddressList(addresses))
                } else {
                    Ok(AddressFieldsLegacy::AddressField(
                        AddressFieldLegacy::Address(s),
                    ))
                }
            }
            // Single address object
            Value::Object(_) => {
                let addr =
                    AddressFieldLegacy::deserialize(value).map_err(serde::de::Error::custom)?;
                Ok(AddressFieldsLegacy::AddressField(addr))
            }
            // Array of addresses
            Value::Array(arr) => {
                let addresses: Result<Vec<AddressFieldLegacy>, _> = arr
                    .into_iter()
                    .map(|v| AddressFieldLegacy::deserialize(v).map_err(serde::de::Error::custom))
                    .collect();
                Ok(AddressFieldsLegacy::AddressList(addresses?))
            }
            _ => Err(serde::de::Error::custom(
                "expected string, object, or array",
            )),
        }
    }
}

impl TryFrom<&AddressFieldsLegacy> for Vec<String> {
    type Error = Error;

    fn try_from(value: &AddressFieldsLegacy) -> Result<Self, Self::Error> {
        match value {
            AddressFieldsLegacy::AddressField(addr) => Ok(vec![addr.try_into()?]),
            AddressFieldsLegacy::AddressList(list) => {
                list.into_iter().map(|a| a.try_into()).collect()
            }
        }
    }
}

impl TryFrom<AddressFieldsLegacy> for Vec<String> {
    type Error = Error;

    fn try_from(value: AddressFieldsLegacy) -> Result<Self, Self::Error> {
        (&value).try_into()
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

#[derive(serde::Deserialize, Clone)]
pub struct EmailRequestLegacy {
    pub key: String,
    #[serde(default)]
    pub template: EmailTemplateTypeLegacy,
    pub from: AddressFieldLegacy,
    #[serde(rename = "replyTo")]
    pub reply_to: Option<AddressFieldsLegacy>,
    pub to: Option<AddressFieldsLegacy>,
    pub subject: String,
    pub content: Option<String>,
    pub html: Option<String>,
    pub cc: Option<AddressFieldsLegacy>,
    pub bcc: Option<AddressFieldsLegacy>,
    #[serde(rename = "attachments[]")]
    pub attachments: Option<Vec<AttachmentLegacy>>,
}

impl Debug for EmailRequestLegacy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmailRequestLegacy")
            .field("key", &"<hidden>")
            .field("template", &self.template)
            .field("from", &self.from)
            .field("reply_to", &self.reply_to)
            .field("to", &self.to)
            .field("subject", &self.subject)
            .field("content", &self.content)
            .field("html", &self.html)
            .field("cc", &self.cc)
            .field("bcc", &self.bcc)
            .field("attachments", &self.attachments)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_html() {
        let json = r#"{
            "key": "mykey123",
            "from": "sender@datasektionen.se",
            "subject": "Hello World",
            "html": "<p>Test email</p>"
        }"#;
        let req: EmailRequestLegacy = serde_json::from_str(json).unwrap();
        assert_eq!(req.key, "mykey123");
        assert_eq!(req.subject, "Hello World");
        assert_eq!(req.html.unwrap(), "<p>Test email</p>");
        assert_eq!(req.template, EmailTemplateTypeLegacy::Default);
    }

    #[test]
    fn with_content() {
        let json = r#"{
            "key": "mykey123",
            "from": "sender@datasektionen.se",
            "subject": "Hello World",
            "content": "This is plain text"
        }"#;
        let req: EmailRequestLegacy = serde_json::from_str(json).unwrap();
        assert_eq!(req.content.unwrap(), "This is plain text");
        assert!(req.html.is_none());
    }

    #[test]
    fn valid_sender() {
        let json = r#"{
            "key": "mykey123",
            "from": {"name": "John Doe", "address": "john@datasektionen.se"},
            "subject": "Hello",
            "html": "<p>Test</p>"
        }"#;
        let req: EmailRequestLegacy = serde_json::from_str(json).unwrap();
        match &req.from {
            AddressFieldLegacy::NameAndAddress(na) => {
                assert_eq!(na.name, "John Doe");
                assert_eq!(na.address, "john@datasektionen.se");
            }
            _ => panic!("Expected NameAndAddress"),
        }
    }

    #[test]
    fn valid_recipient() {
        let json = r#"{
            "key": "mykey123",
            "from": "sender@datasektionen.se",
            "to": "recipient@datasektionen.se",
            "subject": "Hello",
            "html": "<p>Test</p>"
        }"#;
        let req: EmailRequestLegacy = serde_json::from_str(json).unwrap();
        assert!(req.to.is_some());
    }

    #[test]
    fn valid_recipients() {
        let json = r#"{
            "key": "mykey123",
            "from": "sender@datasektionen.se",
            "to": ["recipient1@datasektionen.se", "recipient2@datasektionen.se"],
            "subject": "Hello",
            "html": "<p>Test</p>"
        }"#;
        let req: EmailRequestLegacy = serde_json::from_str(json).unwrap();
        let recipients: Vec<String> = req.to.unwrap().try_into().unwrap();
        assert_eq!(recipients.len(), 2);
    }

    #[test]
    fn valid_cc_and_bcc() {
        let json = r#"{
            "key": "mykey123",
            "from": "sender@datasektionen.se",
            "to": "recipient@datasektionen.se",
            "cc": ["cc@datasektionen.se"],
            "bcc": ["bcc1@datasektionen.se", "Bcc <bcc2@datasektionen.se>"],
            "subject": "Hello",
            "html": "<p>Test</p>"
        }"#;
        let req: EmailRequestLegacy = serde_json::from_str(json).unwrap();
        assert!(req.cc.is_some());
        assert!(req.bcc.is_some());
        let bcc: Vec<String> = req.bcc.unwrap().try_into().unwrap();
        assert_eq!(bcc.len(), 2)
    }

    #[test]
    fn valid_reply_to() {
        let json = r#"{
            "key": "mykey123",
            "from": "sender@datasektionen.se",
            "replyTo": "reply@datasektionen.se",
            "subject": "Hello",
            "html": "<p>Test</p>"
        }"#;
        let req: EmailRequestLegacy = serde_json::from_str(json).unwrap();
        assert!(req.reply_to.is_some());
    }

    #[test]
    fn valid_template() {
        let json = r#"{
            "key": "mykey123",
            "from": "sender@datasektionen.se",
            "template": "metaspexet",
            "subject": "Hello",
            "html": "<p>Test</p>"
        }"#;
        let req: EmailRequestLegacy = serde_json::from_str(json).unwrap();
        assert_eq!(req.template, EmailTemplateTypeLegacy::Metaspexet);
    }

    #[test]
    fn valid_attachments() {
        let json = r#"{
            "key": "mykey123",
            "from": "sender@datasektionen.se",
            "subject": "With attachment",
            "html": "<p>See attachment</p>",
            "attachments[]": [
                {
                    "originalname": "document.pdf",
                    "mimetype": "application/pdf",
                    "buffer": "JVBERi0xLjQ="
                }
            ]
        }"#;
        let req: EmailRequestLegacy = serde_json::from_str(json).unwrap();
        assert!(req.attachments.is_some());
        let attachments = req.attachments.unwrap();
        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].original_name, "document.pdf");
    }

    #[test]
    fn valid_empty_attachments() {
        let json = r#"{
            "key": "mykey123",
            "from": "sender@datasektionen.se",
            "subject": "No attachments",
            "attachments[]": []
        }"#;
        let req: EmailRequestLegacy = serde_json::from_str(json).unwrap();
        let attachments = req.attachments.unwrap();
        assert_eq!(attachments.len(), 0);
    }

    #[test]
    fn valid_string_addresses() {
        let json = r#"{
            "key": "mykey123",
            "from": "sender@datasektionen.se",
            "to": "recipient@datasektionen.se, test <other@datasektionen.se>",
            "subject": "Multiple recipients"
        }"#;
        let req: EmailRequestLegacy = serde_json::from_str(json).unwrap();
        let to: Vec<String> = req.to.unwrap().try_into().unwrap();
        assert_eq!(to.len(), 2);
    }

    #[test]
    fn valid_fancy_address() {
        let json = r#"{
            "key": "mykey123",
            "from": "Test <sender@datasektionen.se>",
            "to": ["Test <recipient@datasektionen.se>", "Other <other@datasektionen.se>"],
            "subject": "Multiple recipients"
        }"#;
        let req: EmailRequestLegacy = serde_json::from_str(json).unwrap();
        let to: Vec<String> = req.to.unwrap().try_into().unwrap();
        assert_eq!(to.len(), 2);
    }

    #[test]
    fn valid_utf8_address() {
        let json = r#"{
            "key": "mykey123",
            "from": {"name": "åäö", "address": "sender@datasektionen.se"},
            "subject": "Hello"
        }"#;
        let req: EmailRequestLegacy = serde_json::from_str(json).unwrap();
        let from: String = req.from.try_into().unwrap();
        assert_eq!(from, "=?UTF-8?B?w6XDpMO2?= <sender@datasektionen.se>");
    }

    #[test]
    fn valid_utf8_fancy_address() {
        let json = r#"{
            "key": "mykey123",
            "from": "åäö <sender@datasektionen.se>",
            "to": "åäö <recipient@datasektionen.se>, åäö <other@datasektionen.se>",
            "subject": "Hello"
        }"#;
        let req: EmailRequestLegacy = serde_json::from_str(json).unwrap();
        let from: String = req.from.try_into().unwrap();
        assert_eq!(from, "=?UTF-8?B?w6XDpMO2?= <sender@datasektionen.se>");

        let to: Vec<String> = req.to.unwrap().try_into().unwrap();
        assert_eq!(to.len(), 2);
        assert_eq!(
            to.get(0).unwrap(),
            "=?UTF-8?B?w6XDpMO2?= <recipient@datasektionen.se>"
        );
        assert_eq!(
            to.get(1).unwrap(),
            "=?UTF-8?B?w6XDpMO2?= <other@datasektionen.se>"
        );
    }
}
