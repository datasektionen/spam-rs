use std::fmt::Display;

use actix_cors::Cors;
use actix_web::http::{Method, StatusCode};
use actix_web::middleware::Logger;
use actix_web::web::scope;
use actix_web::{App, HttpServer, ResponseError, post};
use actix_web::{HttpResponse, web};
use aws_config::BehaviorVersion;
use aws_sdk_sesv2 as sesv2;
use aws_sdk_sesv2::types::builders::AttachmentBuilder;
use aws_sdk_sesv2::types::{
    Attachment, AttachmentContentTransferEncoding, Body, Content, Destination, EmailContent,
    Message,
};
use base64::prelude::*;
use log::{error, info};
use std::path::Path;
use std::{env, fs};

#[derive(serde::Deserialize, Debug, Clone, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "lowercase")]
enum EmailTemplateTypeLegacy {
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum VerifiedDomains {
    Metaspexet,
    Datasektionen,
    Ddagen,
}

impl TryFrom<String> for VerifiedDomains {
    type Error = Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "metaspexet.se" => Ok(VerifiedDomains::Metaspexet),
            "datasektionen.se" => Ok(VerifiedDomains::Datasektionen),
            "ddagen.se" => Ok(VerifiedDomains::Ddagen),
            _ => Err(Error::InvalidEmailDomain(value)),
        }
    }
}

#[derive(serde::Serialize, Debug, Clone)]
struct ContentData {
    is_html: bool,
    content: String,
}

#[derive(serde::Deserialize, Debug, Clone)]
struct EmailNameLegacy {
    name: String,
    address: String,
}

#[derive(serde::Deserialize, Debug, Clone)]
struct AttachmentLegacy {
    originalname: String,
    mimetype: String,
    buffer: String,
    encoding: String,
}

#[derive(serde::Deserialize, Debug, Clone)]
struct EmailRequestLegacy {
    key: String,
    #[serde(default)]
    template: EmailTemplateTypeLegacy,
    from: EmailNameLegacy,
    #[serde(rename = "replyTo")]
    reply_to: Option<String>,
    to: Vec<String>,
    subject: String,
    content: Option<String>,
    html: Option<String>,
    cc: Option<Vec<EmailNameLegacy>>,
    bcc: Option<Vec<String>>,
    #[serde(rename = "attachments[]")]
    attachments: Option<Vec<AttachmentLegacy>>,
}

#[derive(Debug)]
enum Error {
    EnvVarMissing(String),
    InvalidEmailDomain(String),
    ApiKeyInvalid,
    ApiKeyLookup(String),
    MissingContent,
    EmailSend(String),
    TemplateRender(String),
    TemplateLoad(String),
    Attachment(String),
    EmailBody(String),
}

impl From<sesv2::Error> for Error {
    fn from(err: sesv2::Error) -> Self {
        Error::EmailSend(err.to_string())
    }
}

impl From<handlebars::RenderError> for Error {
    fn from(err: handlebars::RenderError) -> Self {
        Error::TemplateRender(err.to_string())
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::TemplateLoad(err.to_string())
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::EnvVarMissing(msg) => write!(f, "Environment variable missing: {}", msg),
            Error::ApiKeyInvalid => write!(f, "API key is invalid or lacks permissions"),
            Error::ApiKeyLookup(msg) => write!(f, "API lookup failed: {}", msg),
            Error::InvalidEmailDomain(domain) => write!(f, "Invalid email domain: {}", domain),
            Error::EmailSend(msg) => write!(f, "Failed to send email: {}", msg),
            Error::TemplateRender(msg) => write!(f, "Failed to render template: {}", msg),
            Error::TemplateLoad(msg) => write!(f, "Failed to load template: {}", msg),
            Error::Attachment(msg) => write!(f, "Failed to process attachment: {}", msg),
            Error::EmailBody(msg) => write!(f, "Failed to process email body: {}", msg),
            Error::MissingContent => write!(f, "No 'html' or 'content' field provided."),
        }
    }
}

impl From<&Error> for HttpResponse {
    fn from(val: &Error) -> Self {
        match val {
            Error::ApiKeyInvalid => HttpResponse::Unauthorized().body(val.to_string()),
            Error::EmailSend(_)
            | Error::TemplateRender(_)
            | Error::TemplateLoad(_)
            | Error::ApiKeyLookup(_)
            | Error::EnvVarMissing(_) => HttpResponse::InternalServerError().body(val.to_string()),
            Error::Attachment(_)
            | Error::EmailBody(_)
            | Error::InvalidEmailDomain(_)
            | Error::MissingContent => HttpResponse::BadRequest().body(val.to_string()),
        }
    }
}

impl ResponseError for Error {
    fn error_response(&self) -> HttpResponse {
        HttpResponse::from(self)
    }
    fn status_code(&self) -> actix_web::http::StatusCode {
        match self {
            Error::ApiKeyInvalid => StatusCode::UNAUTHORIZED,
            Error::EmailSend(_)
            | Error::TemplateRender(_)
            | Error::TemplateLoad(_)
            | Error::ApiKeyLookup(_)
            | Error::EnvVarMissing(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Error::Attachment(_)
            | Error::EmailBody(_)
            | Error::InvalidEmailDomain(_)
            | Error::MissingContent => StatusCode::BAD_REQUEST,
        }
    }
}

#[derive(Clone, Debug)]
struct Client {
    inner: sesv2::Client,
    templates: handlebars::Handlebars<'static>,
}

fn load_template_file(template_name: &str) -> Result<String, std::io::Error> {
    let path = Path::new("templates").join(template_name);
    let content = fs::read_to_string(path)?;
    Ok(content)
}

impl Client {
    async fn new() -> Self {
        let config = aws_config::load_defaults(BehaviorVersion::latest())
            .await
            .into_builder()
            .build();
        let inner = sesv2::Client::new(&config);
        let templates = handlebars::Handlebars::new();
        Self { inner, templates }
    }

    async fn send_email_legacy(&self, mail: EmailRequestLegacy) -> Result<String, Error> {
        let domain = match mail.from.address.split('@').nth(1) {
            Some(d) => d,
            None => {
                return Err(Error::InvalidEmailDomain(mail.from.address));
            }
        };

        match VerifiedDomains::try_from(domain.to_string()) {
            Ok(_) => {}
            Err(_) => {
                return Err(Error::InvalidEmailDomain(domain.to_string()));
            }
        };

        let cc = mail.cc.as_ref().map(|cc_list| {
            cc_list
                .iter()
                .map(|cc| cc.address.clone())
                .collect::<Vec<String>>()
        });

        let content = if let Some(html) = &mail.html {
            html
        } else if let Some(text) = &mail.content {
            text
        } else {
            ""
        };
        let is_html = mail.html.is_some();

        // Build the destination
        let dest = Destination::builder()
            .set_to_addresses(Some(mail.to))
            .set_cc_addresses(cc)
            .set_bcc_addresses(mail.bcc)
            .build();

        // Build subject content
        let subj = Content::builder()
            .data(mail.subject)
            .charset("UTF-8")
            .build()
            .map_err(|e| Error::EmailSend(format!("Failed to build subject content: {}", e)))?;

        let body_text = if mail.template != EmailTemplateTypeLegacy::None {
            match self.render_template(&mail.template, content.to_string(), is_html) {
                Ok(rendered) => rendered,
                Err(e) => {
                    error!("Failed to render template: {}", e);
                    content.to_string()
                }
            }
        } else if !is_html {
            markdown::to_html(content)
        } else {
            content.to_string()
        };

        // Build body content
        let body = Body::builder()
            .html(
                Content::builder()
                    .data(body_text)
                    .charset("UTF-8")
                    .build()
                    .map_err(|e| {
                        Error::EmailBody(format!("Failed to build body content: {}", e))
                    })?,
            )
            .build();

        let attachments: Option<Vec<Attachment>> = mail
            .attachments
            .map(|atts| {
                atts.iter()
                    .map(|att| {
                        if att.encoding != "base64" {
                            return Err(Error::Attachment(format!(
                                "Unsupported attachment encoding: {}",
                                att.encoding
                            )));
                        }
                        let data = BASE64_STANDARD.decode(&att.buffer).map_err(|e| {
                            Error::Attachment(format!(
                                "Failed to decode attachment {}: {}",
                                att.originalname, e
                            ))
                        })?;
                        AttachmentBuilder::default()
                            .raw_content(data.into())
                            .file_name(att.originalname.clone())
                            .content_type(att.mimetype.clone())
                            .content_transfer_encoding(AttachmentContentTransferEncoding::Base64)
                            .build()
                            .map_err(|e| {
                                Error::Attachment(format!(
                                    "Failed to build attachment {}: {}",
                                    att.originalname, e
                                ))
                            })
                    })
                    .collect::<Result<Vec<Attachment>, Error>>()
            })
            .transpose()?;

        let message = Message::builder()
            .subject(subj)
            .body(body)
            .set_attachments(attachments)
            .build();

        let email_content = EmailContent::builder().simple(message).build();

        let resp = self
            .inner
            .send_email()
            .from_email_address(mail.from.address)
            .destination(dest)
            .set_reply_to_addresses(mail.reply_to.map(|a| vec![a]))
            .content(email_content)
            .send()
            .await
            .map_err(|e| Error::EmailSend(format!("Email failed to send: {}", e)))?;

        // The response includes a message ID (if accepted)
        let message_id = resp.message_id().map(|s| s.to_string()).unwrap_or_default();

        Ok(message_id)
    }

    fn load_templates(&mut self) -> Result<(), Error> {
        let template_files = vec![
            (EmailTemplateTypeLegacy::Default, "default/html.hbs"),
            (EmailTemplateTypeLegacy::Metaspexet, "metaspexet/html.hbs"),
        ];

        for (template_type, file_name) in template_files {
            match load_template_file(file_name) {
                Ok(template_content) => {
                    self.templates
                        .register_template_string(&template_type.to_string(), template_content)
                        .map_err(|e| {
                            Error::TemplateLoad(format!(
                                "Failed to register template {}: {}",
                                file_name, e
                            ))
                        })?;
                }
                Err(e) => {
                    return Err(Error::TemplateLoad(format!(
                        "Failed to load template file {}: {}",
                        file_name, e
                    )));
                }
            };
        }

        Ok(())
    }

    fn render_template(
        &self,
        template: &EmailTemplateTypeLegacy,
        content: String,
        is_html: bool,
    ) -> Result<String, handlebars::RenderError> {
        let content = if is_html {
            content
        } else {
            markdown::to_html(&content)
        };
        let data = ContentData { is_html, content };
        let rendered = self.templates.render(&template.to_string(), &data)?;
        info!("Rendered template: {}", rendered);
        Ok(rendered)
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    println!("starting");
    env_logger::init();

    let address = env::var("HOST_ADDRESS").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = env::var("PORT")
        .unwrap_or_else(|_| "8000".to_string())
        .parse::<u16>()
        .unwrap_or(8000);

    let mut client = Client::new().await;
    match client.load_templates() {
        Ok(_) => {
            info!("Templates loaded successfully");
        }
        Err(e) => {
            error!("{}", e);
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            ));
        }
    }
    let client = web::Data::new(client);

    HttpServer::new(move || {
        let cors = Cors::default()
            .allow_any_header()
            .allowed_methods([Method::POST])
            .allow_any_origin();
        App::new()
            .wrap(cors)
            .wrap(Logger::default())
            .app_data(client.clone())
            .service(scope("/api").service(scope("/legacy").service(send_mail_legacy)))
    })
    .bind((address, port))?
    .run()
    .await
}

#[post("/sendmail")]
async fn send_mail_legacy(ses: web::Data<Client>, body: String) -> Result<HttpResponse, Error> {
    let body = serde_json::from_str::<EmailRequestLegacy>(&body)
        .map_err(|e| Error::EmailBody(format!("Failed to parse email request body: {}", e)))?;

    let hive_url = env::var("HIVE_URL")
        .map_err(|e| Error::EnvVarMissing(format!("HIVE_URL missing: {}", e)))?;

    let client = reqwest::Client::new();
    let res = client
        .get(format!("{}/token/{}/permission/send", hive_url, &body.key))
        .bearer_auth(
            env::var("HIVE_SECRET").map_err(|_| Error::EnvVarMissing("HIVE_SECRET".to_string()))?,
        )
        .send()
        .await
        .map_err(|e| Error::ApiKeyLookup(e.to_string()))?
        .text()
        .await
        .map_err(|e| Error::ApiKeyLookup(e.to_string()))?;

    let is_auth = res
        .trim()
        .parse::<bool>()
        .map_err(|e| Error::ApiKeyLookup(format!("Key parse failed: {}", e)))?;

    if !is_auth {
        return Err(Error::ApiKeyInvalid);
    }

    if body.html.is_none() && body.content.is_none() {
        return Err(Error::MissingContent);
    }

    ses.send_email_legacy(body.clone())
        .await
        .map(|message_id| HttpResponse::Ok().body(format!("{}", message_id)))
}
