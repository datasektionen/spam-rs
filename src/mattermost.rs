use crate::error::Error;
use crate::hive_authenticate_request;
use actix_web::{HttpResponse, post, web};
use serde_json::json;
use std::env;
use url::Url;

#[derive(serde::Deserialize, Clone)]
pub struct MattermostUser {
    pub id: String,
}

#[derive(serde::Deserialize, Clone)]
pub struct MattermostChannel {
    pub id: String,
}

const ALLOWED_HOSTS_ENV: &str = "MATTERMOST_ALLOWED_HOSTS";

fn host_allowed(host: &Url, allowed: &str) -> bool {
    allowed
        .split(',')
        .filter_map(|entry| Url::parse(entry.trim()).ok())
        .any(|entry| entry.origin() == host.origin())
}

#[derive(serde::Deserialize, Clone)]
pub struct NotificationRequest {
    host: Url,
    user_email: String,
    bot_token: String,
    key: String,
    title: String,
    body: String,
    thumbnail: Option<String>,
    author_name: Option<String>,
    author_icon: Option<Url>,
}

#[derive(serde::Deserialize, Clone)]
pub struct ChannelPostRequest {
    host: Url,
    bot_token: String,
    key: String,
    body: String,
    channel: String,
}

#[derive(serde::Deserialize, Clone)]
pub struct DirecetMessageRequest {
    host: Url,
    bot_token: String,
    key: String,
    body: String,
    user_email: String,
}

pub struct PostRequest {
    endpoint: Url,
    channel_id: String,
    bot_token: String,
    message: String,
    props: Option<PostProps>,
}

impl PostRequest {
    fn body(&self) -> PostBody<'_> {
        PostBody {
            channel_id: &self.channel_id,
            message: &self.message,
            props: &self.props,
        }
    }
}

#[derive(serde::Serialize)]
// See https://developers.mattermost.com/integrate/reference/message-attachments/
struct Attachment {
    fallback: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    color: Option<String>,
    text: String,
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thumb_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    author_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    author_icon: Option<Url>,
}

#[derive(serde::Serialize)]
struct PostProps {
    attachments: Vec<Attachment>,
}

#[derive(serde::Serialize)]
struct PostBody<'a> {
    channel_id: &'a String,
    message: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    props: &'a Option<PostProps>,
}

impl PostRequest {
    pub async fn send(self) -> Result<reqwest::Response, Error> {
        let client = reqwest::Client::new();
        let res = client
            .post(self.endpoint.clone())
            .bearer_auth(&self.bot_token)
            .json(&self.body())
            .send()
            .await
            .map_err(|e| Error::MattermostSend(e.to_string()))?;

        Ok(res)
    }
}

pub struct PostRequestBuilder {
    host: Url,
    channel_id: Option<String>,
    bot_token: Option<String>,
    message: Option<String>,
    props: Option<PostProps>,
}

impl PostRequestBuilder {
    pub fn new(host: &Url) -> PostRequestBuilder {
        PostRequestBuilder {
            host: host.clone(),
            channel_id: None,
            bot_token: None,
            message: None,
            props: None,
        }
    }

    pub fn to_channel(self, channel_id: &str) -> PostRequestBuilder {
        PostRequestBuilder {
            channel_id: Some(channel_id.into()),
            ..self
        }
    }

    pub fn using_bot(self, bot_token: &str) -> PostRequestBuilder {
        PostRequestBuilder {
            bot_token: Some(bot_token.into()),
            ..self
        }
    }

    pub fn with_attachment(
        self,
        title: &str,
        text: &str,
        thumb_url: Option<&str>,
        author: Option<&str>,
        author_icon: Option<&Url>,
    ) -> PostRequestBuilder {
        PostRequestBuilder {
            props: self.props.map_or_else(
                || {
                    Some(PostProps {
                        attachments: vec![Attachment {
                            title: Some(title.into()),
                            text: text.into(),
                            fallback: text.into(),
                            color: None,
                            thumb_url: thumb_url.map(Into::into),
                            author_name: author.map(Into::into),
                            author_icon: author_icon.cloned(),
                        }],
                    })
                },
                |props| {
                    let mut attachments = props.attachments;
                    attachments.push(Attachment {
                        title: Some(title.into()),
                        text: text.into(),
                        fallback: text.into(),
                        color: None,
                        thumb_url: thumb_url.map(Into::into),
                        author_name: author.map(Into::into),
                        author_icon: author_icon.cloned(),
                    });
                    Some(PostProps { attachments })
                },
            ),
            ..self
        }
    }

    pub fn with_message(self, message: &str) -> PostRequestBuilder {
        PostRequestBuilder {
            message: Some(message.into()),
            ..self
        }
    }

    pub fn build(self) -> Result<PostRequest, Error> {
        let endpoint = self
            .host
            .join("/api/v4/posts")
            .map_err(|e| Error::InvalidAddress(e.to_string()))?;

        Ok(PostRequest {
            endpoint,
            bot_token: self.bot_token.ok_or(Error::MissingBotToken)?,
            channel_id: self.channel_id.ok_or(Error::MissingChannel)?,
            message: self.message.ok_or(Error::MissingMessage)?,
            props: self.props,
        })
    }
}

pub struct MattermostClient {
    host: Url,
}

impl MattermostClient {
    pub fn new(host: Url) -> MattermostClient {
        MattermostClient { host: host.clone() }
    }

    pub fn send_post(self) -> PostRequestBuilder {
        PostRequestBuilder::new(&self.host)
    }
}

async fn get_dm_channel(user_email: &str, host: &Url, bot_token: &str) -> Result<String, Error> {
    let client = reqwest::Client::new();
    let user = client
        .get(
            host.join(format!("/api/v4/users/email/{}", user_email).as_str())
                .map_err(|e| Error::InvalidAddress(e.to_string()))?,
        )
        .bearer_auth(bot_token)
        .send()
        .await
        .map_err(|e| Error::MattermostSend(e.to_string()))?
        .error_for_status()
        .map_err(|e| Error::MattermostSend(e.to_string()))?
        .json::<MattermostUser>()
        .await
        .map_err(|e| Error::Deserialization(e.to_string()))?;

    let bot = client
        .get(
            host.join("/api/v4/users/me")
                .map_err(|e| Error::InvalidAddress(e.to_string()))?,
        )
        .bearer_auth(bot_token)
        .send()
        .await
        .map_err(|e| Error::MattermostSend(e.to_string()))?
        .error_for_status()
        .map_err(|e| Error::MattermostSend(e.to_string()))?
        .json::<MattermostUser>()
        .await
        .map_err(|e| Error::Deserialization(e.to_string()))?;

    // Create the dm channel
    let res = client
        .post(
            host.join("/api/v4/channels/direct")
                .map_err(|e| Error::InvalidAddress(e.to_string()))?,
        )
        .bearer_auth(bot_token)
        .json(&json!([user.id, bot.id]))
        .send()
        .await
        .map_err(|e| Error::MattermostSend(e.to_string()))?
        .error_for_status()
        .map_err(|e| Error::MattermostSend(e.to_string()))?
        .json::<MattermostChannel>()
        .await
        .map_err(|e| Error::Deserialization(e.to_string()))?;

    Ok(res.id)
}

/// Sends a formatted "Notification" to a user's DM channel.
///
/// Will format the title/body as a message attachment (will render as an outlined box)
#[post("/notify")]
pub async fn send_notification(
    body: web::Json<NotificationRequest>,
) -> Result<HttpResponse, Error> {
    hive_authenticate_request(&body.key).await?;

    let allowed = env::var(ALLOWED_HOSTS_ENV)
        .map_err(|_| Error::EnvVarMissing(ALLOWED_HOSTS_ENV.to_string()))?;
    if !host_allowed(&body.host, &allowed) {
        return Err(Error::HostNotAllowed(body.host.to_string()));
    }

    let channel = get_dm_channel(&body.user_email, &body.host, &body.bot_token).await?;

    let res = PostRequestBuilder::new(&body.host)
        .with_attachment(
            &body.title,
            &body.body,
            body.thumbnail.as_deref(),
            body.author_name.as_deref(),
            body.author_icon.as_ref(),
        ) // deref keeps Option
        .using_bot(&body.bot_token)
        .to_channel(&channel)
        .with_message("")
        .build()?
        .send()
        .await?;
    if res.status().is_success() {
        Ok(HttpResponse::Ok().body("Post sent successfully"))
    } else {
        Err(Error::MattermostSend(format!(
            "Failed to send post: {}",
            res.status()
        )))
    }
}

/// Send a post to a channel using its ID.
///
/// This can be any channel (including a DM channel) if you have the ID.
#[post("/channel")]
pub async fn send_to_channel(body: web::Json<ChannelPostRequest>) -> Result<HttpResponse, Error> {
    hive_authenticate_request(&body.key).await?;

    let allowed =
        env::var(ALLOWED_HOSTS_ENV).map_err(|_| Error::EnvVarMissing(ALLOWED_HOSTS_ENV.into()))?;
    if !host_allowed(&body.host, &allowed) {
        return Err(Error::HostNotAllowed(body.host.to_string()));
    }

    let res = PostRequestBuilder::new(&body.host)
        .using_bot(&body.bot_token)
        .with_message(&body.body)
        .to_channel(&body.channel)
        .build()?
        .send()
        .await?;

    if res.status().is_success() {
        Ok(HttpResponse::Ok().body("Post sent successfully"))
    } else {
        Err(Error::MattermostSend(format!(
            "Failed to send post: {}",
            res.status()
        )))
    }
}

/// Send a post to a DM channel given a user email.
///
/// Will find/create the necessary channel, so no channel id needs to be passed.
#[post("/dm")]
pub async fn send_dm(body: web::Json<DirecetMessageRequest>) -> Result<HttpResponse, Error> {
    hive_authenticate_request(&body.key).await?;

    let allowed =
        env::var(ALLOWED_HOSTS_ENV).map_err(|_| Error::EnvVarMissing(ALLOWED_HOSTS_ENV.into()))?;
    if !host_allowed(&body.host, &allowed) {
        return Err(Error::HostNotAllowed(body.host.to_string()));
    }

    let dm_channel = get_dm_channel(&body.user_email, &body.host, &body.bot_token).await?;

    let res = PostRequestBuilder::new(&body.host)
        .using_bot(&body.bot_token)
        .to_channel(&dm_channel)
        .with_message(&body.body)
        .build()?
        .send()
        .await?;

    if res.status().is_success() {
        Ok(HttpResponse::Ok().body("Post sent successfully"))
    } else {
        Err(Error::MattermostSend(format!(
            "Failed to send post: {}",
            res.status()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::assert_matches;

    #[test]
    fn test_builder() {
        let post = PostRequestBuilder::new(&Url::parse("https://example.com/").unwrap())
            .to_channel("xyz")
            .using_bot("abc")
            .with_message("text")
            .build();

        assert!(post.is_ok());

        let Ok(post) = post else {
            panic!("Expected Ok")
        };

        assert_eq!(post.endpoint.as_str(), "https://example.com/api/v4/posts");
        assert_eq!(post.channel_id.as_str(), "xyz");
        assert_eq!(post.bot_token.as_str(), "abc");
        assert_eq!(post.message.as_str(), "text");
    }

    #[test]
    fn test_builder_missing_fields() {
        // Missing bot_token
        let post = PostRequestBuilder::new(&Url::parse("https://example.com/").unwrap())
            .to_channel("xyz")
            .with_message("Hello world")
            .build();

        assert!(post.is_err());
        assert_matches!(post.err(), Some(Error::MissingBotToken));

        // Missing content
        let post = PostRequestBuilder::new(&Url::parse("https://example.com/").unwrap())
            .to_channel("xyz")
            .using_bot("abc")
            .build();

        assert!(post.is_err());
        assert_matches!(post.err(), Some(Error::MissingMessage));

        // Missing channel
        let post = PostRequestBuilder::new(&Url::parse("https://example.com/").unwrap())
            .using_bot("abc")
            .with_message("text")
            .build();

        assert!(post.is_err());
        assert_matches!(post.err(), Some(Error::MissingChannel));
    }

    #[test]
    fn test_host_allowed() {
        let allowed = "http://mattermost:8065, https://chat.example.com";

        assert!(host_allowed(
            &Url::parse("http://mattermost:8065").unwrap(),
            allowed
        ));
        assert!(host_allowed(
            &Url::parse("http://mattermost:8065/api/v4/posts").unwrap(),
            allowed
        ));
        assert!(host_allowed(
            &Url::parse("https://chat.example.com/").unwrap(),
            allowed
        ));

        assert!(!host_allowed(
            &Url::parse("http://mattermost:9000").unwrap(),
            allowed
        ));
        assert!(!host_allowed(
            &Url::parse("https://mattermost:8065").unwrap(),
            allowed
        ));
        assert!(!host_allowed(
            &Url::parse("http://evil.example.com").unwrap(),
            allowed
        ));
    }
}
