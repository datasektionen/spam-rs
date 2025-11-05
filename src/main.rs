use actix_cors::Cors;
use actix_web::http::{self, Method};
use actix_web::middleware::Logger;
use actix_web::web::scope;
use actix_web::{App, HttpServer, post};
use actix_web::{HttpResponse, web};
use aws_config::BehaviorVersion;
use aws_sdk_sesv2 as sesv2;
use aws_sdk_sesv2::types::builders::AttachmentBuilder;
use aws_sdk_sesv2::types::{
    Attachment, AttachmentContentTransferEncoding, Body, Content, Destination, EmailContent,
    Message,
};
use base64::prelude::*;
use log::{debug, error, info};
use std::path::Path;
use std::{env, fs};

mod error;
mod legacy;

use error::Error;
use legacy::email::{AddressFieldLegacy, EmailRequestLegacy, EmailTemplateTypeLegacy};

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
        let from = match &mail.from {
            AddressFieldLegacy::Address(addr) => addr.to_owned(),
            AddressFieldLegacy::NameAndAddress(name_addr) => name_addr.address.to_owned(),
        };

        let domain = from
            .trim()
            .split('@')
            .last()
            .ok_or(Error::InvalidEmailDomain("missing domain".to_string()))?;

        match VerifiedDomains::try_from(domain.to_string()) {
            Ok(_) => {}
            Err(_) => {
                return Err(Error::InvalidEmailDomain(domain.to_string()));
            }
        };

        // After this point, `from` is guaranteed to be a valid email address,
        // but not assuredly ASCII
        let from: String = mail.from.try_into()?;

        let cc = mail
            .cc
            .map(|cc_list| {
                cc_list
                    .to_list()
                    .iter()
                    .map(|cc| cc.try_into())
                    .collect::<Result<Vec<String>, Error>>()
            })
            .transpose()?;

        let content = if let Some(html) = &mail.html {
            Ok(html)
        } else if let Some(text) = &mail.content {
            Ok(text)
        } else {
            Err(Error::MissingContent)
        }?;

        let is_html = mail.html.is_some();

        let to: Option<Vec<String>> = mail
            .to
            .map(|to_list| {
                to_list
                    .to_list()
                    .iter()
                    .map(|to| to.try_into())
                    .collect::<Result<Vec<String>, Error>>()
            })
            .transpose()?;

        let bcc = mail
            .bcc
            .map(|bcc_list| {
                bcc_list
                    .to_list()
                    .iter()
                    .map(|bcc| bcc.try_into())
                    .collect::<Result<Vec<String>, Error>>()
            })
            .transpose()?;

        // Build the destination
        let dest = Destination::builder()
            .set_to_addresses(to)
            .set_cc_addresses(cc)
            .set_bcc_addresses(bcc)
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
                        let data = match att.encoding.as_str() {
                            "base64" | "BASE64" | "Base64" => {
                                BASE64_STANDARD.decode(&att.buffer).map_err(|e| {
                                    Error::Attachment(format!(
                                        "Failed to decode attachment {}: {}",
                                        att.original_name, e
                                    ))
                                })
                            }
                            "utf-8" | "utf8" | "UTF-8" | "UTF8" => {
                                Ok(att.buffer.as_bytes().to_vec())
                            }
                            _ => Err(Error::Attachment(format!(
                                "Unsupported attachment encoding: {}",
                                att.encoding
                            ))),
                        }?;

                        AttachmentBuilder::default()
                            .raw_content(data.into())
                            .file_name(att.original_name.to_owned())
                            .content_type(att.mimetype.to_owned())
                            .content_transfer_encoding(AttachmentContentTransferEncoding::Base64)
                            .build()
                            .map_err(|e| {
                                Error::Attachment(format!(
                                    "Failed to build attachment {}: {}",
                                    att.original_name, e
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
        let reply_to = mail
            .reply_to
            .as_ref()
            .map(|r| String::try_from(r))
            .transpose()?
            .map(|addr| vec![addr]);

        let resp = self
            .inner
            .send_email()
            .from_email_address(from)
            .destination(dest)
            .set_reply_to_addresses(reply_to)
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
        debug!("Rendered template: {}", rendered);
        Ok(rendered)
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    let address = env::var("HOST_ADDRESS").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = env::var("PORT")
        .unwrap_or_else(|_| "8000".to_string())
        .parse::<u16>()
        .unwrap_or(8000);

    let mut client = Client::new().await;
    client
        .load_templates()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
    let client = web::Data::new(client);

    info!("Listening on {}:{}", address, port);
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
async fn send_mail_legacy(
    ses: web::Data<Client>,
    json: Option<web::Json<EmailRequestLegacy>>,
    form: Option<web::Form<EmailRequestLegacy>>,
) -> Result<HttpResponse, Error> {
    let body = if let Some(json) = json {
        Ok(json.into_inner())
    } else if let Some(form) = form {
        Ok(form.into_inner())
    } else {
        Err(Error::InvalidContentType)
    }?;

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

    ses.send_email_legacy(body)
        .await
        .map(|message_id| HttpResponse::Ok().body(format!("{}", message_id)))
}
