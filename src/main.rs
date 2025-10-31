use std::fmt::Display;

use actix_cors::Cors;
use actix_web::web::scope;
use actix_web::{App, HttpServer, post};
use actix_web::{HttpResponse, web};
use aws_config::BehaviorVersion;
use aws_sdk_sesv2 as sesv2;
use aws_sdk_sesv2::operation::send_email::builders::SendEmailFluentBuilder;
use aws_sdk_sesv2::types::builders::AttachmentBuilder;
use aws_sdk_sesv2::types::{
    Attachment, AttachmentContentTransferEncoding, Body, Content, Destination, EmailContent,
    Message,
};
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
    reply_to: Option<Vec<String>>,
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
        }
    }
}

impl From<Error> for HttpResponse {
    fn from(val: Error) -> Self {
        match val {
            Error::ApiKeyInvalid => HttpResponse::Unauthorized().body(val.to_string()),
            Error::EmailSend(_)
            | Error::TemplateRender(_)
            | Error::TemplateLoad(_)
            | Error::ApiKeyLookup(_)
            | Error::EnvVarMissing(_) => HttpResponse::InternalServerError().body(val.to_string()),
            Error::Attachment(_) | Error::EmailBody(_) | Error::InvalidEmailDomain(_) => {
                HttpResponse::BadRequest().body(val.to_string())
            }
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

        let attachments: Vec<Attachment> = mail
            .attachments
            .ok_or_else(|| Error::EmailSend("Attachments are missing".to_string()))?
            .iter()
            .map(|att| {
                if att.encoding != "base64" {
                    return Err(Error::Attachment(format!(
                        "Unsupported attachment encoding: {}",
                        att.encoding
                    )));
                }
                Ok(AttachmentBuilder::default()
                    .raw_content(att.buffer.clone().into_bytes().into())
                    .file_name(att.originalname.clone())
                    .content_type(att.mimetype.clone())
                    .content_transfer_encoding(AttachmentContentTransferEncoding::Base64)
                    .build()
                    .map_err(|e| {
                        Error::Attachment(format!(
                            "Failed to build attachment {}: {}",
                            att.originalname, e
                        ))
                    })?)
            })
            .collect::<Result<Vec<Attachment>, Error>>()?;

        let message = Message::builder()
            .subject(subj)
            .body(body)
            .set_attachments(Some(attachments))
            .build();

        let email_content = EmailContent::builder().simple(message).build();

        let resp = self
            .inner
            .send_email()
            .from_email_address(mail.from.address)
            .destination(dest)
            .set_reply_to_addresses(mail.reply_to)
            .content(email_content)
            .send()
            .await
            .map_err(|e| Error::EmailSend(format!("Email failed to send: {}", e)))?;

        // The response includes a message ID (if accepted)
        let message_id = resp.message_id().map(|s| s.to_string()).unwrap_or_default();

        Ok(message_id)
    }

    fn load_templates(&mut self) {
        let template_files = vec![
            (EmailTemplateTypeLegacy::Default, "default/html.hbs"),
            (EmailTemplateTypeLegacy::Metaspexet, "metaspexet/html.hbs"),
        ];

        for (template_type, file_name) in template_files {
            match load_template_file(file_name) {
                Ok(template_content) => {
                    self.templates
                        .register_template_string(&template_type.to_string(), template_content)
                        .expect("Failed to register template");
                }
                Err(e) => {
                    error!("Failed to load template {}: {}", file_name, e);
                }
            }
        }
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

    let mut client = Client::new().await;
    client.load_templates();
    let client = web::Data::new(client);

    HttpServer::new(move || {
        let cors = Cors::permissive();
        App::new()
            .wrap(cors)
            .app_data(client.clone())
            .service(scope("/api").service(scope("/legacy").service(send_mail_legacy)))
    })
    .bind(("0.0.0.0", 8000))?
    .run()
    .await
}

#[post("/sendmail")]
async fn send_mail_legacy(ses: web::Data<Client>, body: String) -> HttpResponse {
    let body = serde_json::from_str::<EmailRequestLegacy>(&body)
        .map_err(|e| Error::EmailBody(format!("Failed to parse email request body: {}", e)));
    let body = match body {
        Ok(b) => b,
        Err(e) => return HttpResponse::from(e),
    };

    let client = reqwest::Client::new();
    let hive_url = match env::var("HIVE_URL") {
        Ok(url) => url,
        Err(e) => {
            return HttpResponse::from(Error::EnvVarMissing(format!("HIVE_URL missing: {}", e)));
        }
    };
    let res = match client
        .get(format!("{}/token/{}/permission/send", hive_url, &body.key))
        .bearer_auth(env::var("HIVE_SECRET").unwrap())
        .send()
        .await
        .unwrap()
        .text()
        .await
    {
        Ok(text) => text,
        Err(e) => {
            return HttpResponse::from(Error::ApiKeyLookup(format!(
                "Failed to get API key permission: {}",
                e
            )));
        }
    };

    let is_auth = match res
        .trim()
        .parse::<bool>()
        .map_err(|e| Error::ApiKeyLookup(format!("Key parse failed: {}", e)))
    {
        Ok(val) => val,
        Err(e) => {
            return HttpResponse::from(e);
        }
    };
    if !is_auth {
        return HttpResponse::from(Error::ApiKeyInvalid);
    }

    if body.html.is_none() && body.content.is_none() {
        return HttpResponse::BadRequest()
            .body("Either 'html' or 'content' field must be provided.");
    }

    let res = ses.send_email_legacy(body.clone()).await;

    match res {
        Ok(message_id) => HttpResponse::Ok().body(format!(
            "Email sent! Message ID: {}, email request: {:?}",
            message_id, body
        )),
        Err(e) => HttpResponse::from(e),
    }
}
