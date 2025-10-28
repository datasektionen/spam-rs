use std::fmt::Display;

use actix_cors::Cors;
use actix_web::web::scope;
use actix_web::{App, HttpServer, post};
use actix_web::{HttpResponse, web};
use aws_config::BehaviorVersion;
use aws_sdk_sesv2 as sesv2;
use aws_sdk_sesv2::types::builders::AttachmentBuilder;
use aws_sdk_sesv2::types::error::BadRequestException;
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
    to: Vec<String>,
    subject: String,
    content: Option<String>,
    html: Option<String>,
    cc: Option<Vec<EmailNameLegacy>>,
    bcc: Option<Vec<String>>,
    #[serde(rename = "attachments[]")]
    attachments: Option<Vec<AttachmentLegacy>>,
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

    async fn send_email_legacy(&self, mail: EmailRequestLegacy) -> Result<String, sesv2::Error> {
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
            .unwrap();

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
                    .unwrap(),
            )
            .build();

        let attachments: Vec<Attachment> = mail
            .attachments
            .unwrap()
            .iter()
            .map(|att| {
                if att.encoding != "base64" {
                    return Err(sesv2::Error::BadRequestException(
                        BadRequestException::builder()
                            .message("Attachment encoding must be base64".to_string())
                            .build(),
                    ));
                }
                Ok(AttachmentBuilder::default()
                    .raw_content(att.buffer.clone().into_bytes().into())
                    .file_name(att.originalname.clone())
                    .content_type(att.mimetype.clone())
                    .content_transfer_encoding(AttachmentContentTransferEncoding::Base64)
                    .build()
                    .unwrap())
            })
            .collect::<Result<Vec<Attachment>, sesv2::Error>>()?;

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
            .content(email_content)
            .send()
            .await?;

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
        info!("Rendering template {:?} with content: {}", template, content);
        let data = ContentData {
            is_html,
            content,
        };
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
        .map_err(|e| HttpResponse::BadRequest().body(format!("Invalid request body: {}", e)));
    let body = match body {
        Ok(b) => b,
        Err(e) => return e,
    };

    let client = reqwest::Client::new();
    let res = client
        .get(format!(
            "{}/token/{}/permission/send",
            env::var("HIVE_URL").unwrap(),
            &body.key
        ))
        .bearer_auth(env::var("HIVE_SECRET").unwrap())
        .send()
        .await
        .unwrap()
        .text()
        .await;

    let is_auth = res.unwrap().trim().parse::<bool>().unwrap_or(false);
    if !is_auth {
        return HttpResponse::Unauthorized().body("Invalid API key or insufficient permissions.");
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
        Err(e) => HttpResponse::InternalServerError().body(format!("Failed to send email: {}", e)),
    }
}
