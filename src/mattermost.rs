use crate::error::Error;
use crate::hive_authenticate_request;
use actix_web::{post, web, HttpResponse};
use std::env;
use url::Url;

const ALLOWED_HOSTS_ENV: &str = "MATTERMOST_ALLOWED_HOSTS";

fn host_allowed(host: &Url, allowed: &str) -> bool {
    allowed
        .split(',')
        .filter_map(|entry| Url::parse(entry.trim()).ok())
        .any(|entry| entry.origin() == host.origin())
}

#[derive(serde::Deserialize, Clone)]
pub struct MattermostRequest {
    host: Url,
    channel_id: String,
    bot_token: String,
    message: String,
    key: String,
}

pub struct PostRequest {
    endpoint: Url,
    channel_id: String,
    bot_token: String,
    message: String,
}

impl PostRequest {
    fn body(&self) -> PostBody<'_> {
        PostBody {
            channel_id: &self.channel_id,
            message: &self.message,
        }
    }
}

#[derive(serde::Serialize)]
struct PostBody<'a> {
    channel_id: &'a String,
    message: &'a str,
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
    content: Option<String>,
}

impl PostRequestBuilder {
    pub fn new(host: &Url) -> PostRequestBuilder {
        PostRequestBuilder {
            host: host.clone(),
            channel_id: None,
            bot_token: None,
            content: None,
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

    pub fn with_content(self, content: &str) -> PostRequestBuilder {
        PostRequestBuilder {
            content: Some(content.into()),
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
            message: self.content.ok_or(Error::MissingContent)?,
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

#[post("/sendpost")]
pub async fn send_post(body: web::Json<MattermostRequest>) -> Result<HttpResponse, Error> {
    hive_authenticate_request(&body.key).await?;

    let allowed = env::var(ALLOWED_HOSTS_ENV)
        .map_err(|_| Error::EnvVarMissing(ALLOWED_HOSTS_ENV.to_string()))?;
    if !host_allowed(&body.host, &allowed) {
        return Err(Error::HostNotAllowed(body.host.to_string()));
    }

    let res = PostRequestBuilder::new(&body.host)
        .with_content(&body.message)
        .using_bot(&body.bot_token)
        .to_channel(&body.channel_id)
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
            .with_content("text")
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
            .with_content("Hello world")
            .build();

        assert!(post.is_err());
        assert_matches!(post.err(), Some(Error::MissingBotToken));

        // Missing content
        let post = PostRequestBuilder::new(&Url::parse("https://example.com/").unwrap())
            .to_channel("xyz")
            .using_bot("abc")
            .build();

        assert!(post.is_err());
        assert_matches!(post.err(), Some(Error::MissingContent));

        // Missing channel
        let post = PostRequestBuilder::new(&Url::parse("https://example.com/").unwrap())
            .using_bot("abc")
            .with_content("text")
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
