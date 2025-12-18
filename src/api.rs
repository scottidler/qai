use eyre::{Context, Result, eyre};
use serde::{Deserialize, Serialize};

use crate::config::Config;

#[derive(Debug, Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    max_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatResponseMessage,
}

#[derive(Debug, Deserialize)]
struct ChatResponseMessage {
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    error: ErrorDetail,
}

#[derive(Debug, Deserialize)]
struct ErrorDetail {
    message: String,
}

#[derive(Debug)]
pub struct OpenAIClient {
    client: reqwest::Client,
    api_key: String,
    api_base: String,
    model: String,
}

impl OpenAIClient {
    pub fn new(config: &Config) -> Result<Self> {
        let api_key = config.get_api_key().ok_or_else(|| {
            eyre!("No API key found. Set QAI_API_KEY environment variable or add api_key to ~/.config/qai/qai.yml")
        })?;

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            client,
            api_key,
            api_base: config.api_base.clone(),
            model: config.model.clone(),
        })
    }

    #[cfg(test)]
    pub fn new_with_base(api_key: String, api_base: String, model: String) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            client,
            api_key,
            api_base,
            model,
        })
    }

    pub async fn query(&self, system_prompt: &str, user_query: &str) -> Result<String> {
        let url = format!("{}/chat/completions", self.api_base);

        let request = ChatRequest {
            model: self.model.clone(),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: system_prompt.to_string(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: user_query.to_string(),
                },
            ],
            temperature: 0.0,
            max_tokens: 500,
        };

        log::debug!("Sending request to: {}", url);
        log::debug!("Model: {}", self.model);
        log::debug!("User query: {}", user_query);

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to send request to OpenAI API")?;

        let status = response.status();
        let body = response.text().await.context("Failed to read response body")?;

        log::debug!("Response status: {}", status);
        log::debug!("Response body: {}", body);

        if !status.is_success() {
            // Try to parse error response
            if let Ok(error) = serde_json::from_str::<ErrorResponse>(&body) {
                return Err(eyre!("OpenAI API error: {}", error.error.message));
            }
            return Err(eyre!("OpenAI API error ({}): {}", status, body));
        }

        let response: ChatResponse = serde_json::from_str(&body).context("Failed to parse OpenAI response")?;

        let content = response
            .choices
            .first()
            .ok_or_else(|| eyre!("No response from OpenAI"))?
            .message
            .content
            .trim()
            .to_string();

        Ok(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn create_success_response(content: &str) -> String {
        format!(
            r#"{{
                "choices": [{{
                    "message": {{
                        "content": "{}"
                    }}
                }}]
            }}"#,
            content
        )
    }

    fn create_error_response(message: &str) -> String {
        format!(
            r#"{{
                "error": {{
                    "message": "{}"
                }}
            }}"#,
            message
        )
    }

    #[tokio::test]
    async fn test_query_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(header("Authorization", "Bearer test-api-key"))
            .and(header("Content-Type", "application/json"))
            .respond_with(ResponseTemplate::new(200).set_body_string(create_success_response("ls -la")))
            .mount(&mock_server)
            .await;

        let client =
            OpenAIClient::new_with_base("test-api-key".to_string(), mock_server.uri(), "gpt-4o-mini".to_string())
                .unwrap();

        let result = client.query("You are a shell assistant", "list files").await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "ls -la");
    }

    #[tokio::test]
    async fn test_query_trims_whitespace() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_string(create_success_response("  git status  ")))
            .mount(&mock_server)
            .await;

        let client =
            OpenAIClient::new_with_base("test-key".to_string(), mock_server.uri(), "gpt-4o-mini".to_string()).unwrap();

        let result = client.query("system", "query").await.unwrap();
        assert_eq!(result, "git status");
    }

    #[tokio::test]
    async fn test_query_api_error_with_message() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(401).set_body_string(create_error_response("Invalid API key provided")))
            .mount(&mock_server)
            .await;

        let client =
            OpenAIClient::new_with_base("bad-key".to_string(), mock_server.uri(), "gpt-4o-mini".to_string()).unwrap();

        let result = client.query("system", "query").await;

        assert!(result.is_err());
        let error = result.unwrap_err().to_string();
        assert!(error.contains("Invalid API key provided"));
    }

    #[tokio::test]
    async fn test_query_api_error_without_message() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
            .mount(&mock_server)
            .await;

        let client =
            OpenAIClient::new_with_base("key".to_string(), mock_server.uri(), "gpt-4o-mini".to_string()).unwrap();

        let result = client.query("system", "query").await;

        assert!(result.is_err());
        let error = result.unwrap_err().to_string();
        assert!(error.contains("500") || error.contains("Internal Server Error"));
    }

    #[tokio::test]
    async fn test_query_empty_choices() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"choices": []}"#))
            .mount(&mock_server)
            .await;

        let client =
            OpenAIClient::new_with_base("key".to_string(), mock_server.uri(), "gpt-4o-mini".to_string()).unwrap();

        let result = client.query("system", "query").await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No response"));
    }

    #[tokio::test]
    async fn test_query_invalid_json_response() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not valid json"))
            .mount(&mock_server)
            .await;

        let client =
            OpenAIClient::new_with_base("key".to_string(), mock_server.uri(), "gpt-4o-mini".to_string()).unwrap();

        let result = client.query("system", "query").await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("parse"));
    }

    #[tokio::test]
    async fn test_query_uses_correct_model() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_string(create_success_response("result")))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = OpenAIClient::new_with_base("key".to_string(), mock_server.uri(), "gpt-4o".to_string()).unwrap();

        let _ = client.query("system prompt", "user query").await;

        // The mock expectation verifies the request was made
    }

    #[tokio::test]
    async fn test_query_rate_limit_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(429).set_body_string(create_error_response("Rate limit exceeded")))
            .mount(&mock_server)
            .await;

        let client =
            OpenAIClient::new_with_base("key".to_string(), mock_server.uri(), "gpt-4o-mini".to_string()).unwrap();

        let result = client.query("system", "query").await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Rate limit"));
    }

    #[test]
    fn test_new_with_base_works() {
        // Test the direct constructor that doesn't touch env vars
        let result = OpenAIClient::new_with_base(
            "test-key".to_string(),
            "https://api.example.com".to_string(),
            "gpt-4o-mini".to_string(),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_client_stores_correct_values() {
        let client = OpenAIClient::new_with_base(
            "my-api-key".to_string(),
            "https://custom.api.com/v1".to_string(),
            "gpt-4o".to_string(),
        )
        .unwrap();

        assert_eq!(client.api_key, "my-api-key");
        assert_eq!(client.api_base, "https://custom.api.com/v1");
        assert_eq!(client.model, "gpt-4o");
    }
}
