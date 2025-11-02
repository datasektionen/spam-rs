use std::fmt::Display;

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

impl Display for AddressFieldLegacy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AddressFieldLegacy::Address(addr) => write!(f, "{}", addr),
            AddressFieldLegacy::NameAndAddress(name_addr) => {
                write!(f, "{} <{}>", name_addr.name, name_addr.address)
            }
        }
    }
}

#[derive(serde::Deserialize, Debug, Clone)]
pub struct AttachmentLegacy {
    pub originalname: String,
    pub mimetype: String,
    pub buffer: String,
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
