//! Minimal TypeSafe System One HTTP client.
//!
//! The client is intentionally optional. When `TYPESAFE_API_KEY` is absent,
//! callers keep the local retrieval decision path. When configured, the API is
//! used for narrow typed judgments over an already-bounded state snapshot.

use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const DEFAULT_ENDPOINT: &str = "https://api.typesafe.ai/v1/systemone";
pub const DEFAULT_MODEL: &str = "jev-latest";
const DEFAULT_TIMEOUT_MS: u64 = 5_000;
const MAX_TIMEOUT_MS: u64 = 30_000;

#[derive(Debug, Clone)]
pub struct TypeSafeClient {
    http: reqwest::Client,
    endpoint: String,
    api_key: String,
    model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeSafeUsage {
    #[serde(default)]
    pub input_tokens: Option<u64>,
    #[serde(default)]
    pub output_tokens: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeSafeResponse {
    pub model: String,
    #[serde(default)]
    pub answers: Map<String, Value>,
    #[serde(default)]
    pub usage: Option<TypeSafeUsage>,
}

impl TypeSafeClient {
    /// Build a client from the official environment variable convention.
    /// Returns `Ok(None)` when no key is configured, without touching output.
    pub fn from_env() -> Result<Option<Self>> {
        let Some(api_key) = std::env::var("TYPESAFE_API_KEY")
            .ok()
            .filter(|value| !value.trim().is_empty())
        else {
            return Ok(None);
        };

        let endpoint = std::env::var("TYPESAFE_API_URL")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_ENDPOINT.to_owned());
        let model = std::env::var("TYPESAFE_MODEL")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_MODEL.to_owned());
        let timeout_ms = std::env::var("TYPESAFE_TIMEOUT_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(DEFAULT_TIMEOUT_MS)
            .clamp(100, MAX_TIMEOUT_MS);
        let http = reqwest::Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .build()
            .context("build TypeSafe HTTP client")?;

        Ok(Some(Self {
            http,
            endpoint,
            api_key,
            model,
        }))
    }

    #[cfg(test)]
    pub fn with_endpoint_for_test(endpoint: String, api_key: &str) -> Self {
        Self {
            http: reqwest::Client::new(),
            endpoint,
            api_key: api_key.to_owned(),
            model: DEFAULT_MODEL.to_owned(),
        }
    }

    pub async fn evaluate(&self, state: Value, questions: Value) -> Result<TypeSafeResponse> {
        let body = serde_json::json!({
            "state": state,
            "model": self.model,
            "questions": questions,
        });
        let response = self
            .http
            .post(&self.endpoint)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .context("TypeSafe API request")?;
        let status = response.status();
        if !status.is_success() {
            // Do not include the response body: provider errors can echo user
            // state, and credentials must never become part of an error string.
            bail!("TypeSafe API returned HTTP {status}");
        }
        response
            .json::<TypeSafeResponse>()
            .await
            .context("decode TypeSafe API response")
    }
}

/// Questions used by `ask-context`. Each is a small decision that callers can
/// gate independently instead of asking an external model to write prose.
pub fn ask_questions() -> Value {
    serde_json::json!({
        "answerable": {
            "type": "noul",
            "instructions": "Do the retrieved code snippets provide enough direct evidence to answer the user's query?",
            "criteria": {
                "true": "The evidence directly supports a reliable answer.",
                "false": "The evidence is missing, stale, or too indirect."
            }
        },
        "evidence_quality": {
            "type": "score",
            "instructions": "How strong and current is the retrieved code evidence for the user's query?",
            "criteria": [
                "Insufficient or stale evidence",
                "Partial evidence that needs a follow-up",
                "Strong, direct, current evidence"
            ]
        },
        "next_action": {
            "type": "choice",
            "instructions": "Choose the next software action from the retrieved evidence.",
            "criteria": {
                "answer": "Return an answer grounded in the current evidence.",
                "need_more_evidence": "Search, inspect, or fetch more evidence before answering.",
                "retry_index": "Wait for indexing or warming and retry.",
                "no_relevant_evidence": "The indexed workspace does not contain relevant evidence."
            }
        }
    })
}

/// Keep the state sent to the external decision service bounded and free of
/// full source dumps. The caller supplies an already-limited JSON snapshot.
pub fn bounded_state(state: Value, max_chars: usize) -> Value {
    let Ok(serialized) = serde_json::to_string(&state) else {
        return serde_json::json!({ "state": "unserializable" });
    };
    if serialized.chars().count() <= max_chars {
        return state;
    }
    serde_json::json!({
        "truncated_state": serialized.chars().take(max_chars).collect::<String>(),
        "truncated": true
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, extract::Json, http::HeaderMap, routing::post};
    use serde_json::{Value, json};
    use tokio::net::TcpListener;

    #[test]
    fn questions_use_all_system_one_primitives() {
        let questions = ask_questions();
        assert_eq!(questions["answerable"]["type"], "noul");
        assert_eq!(questions["evidence_quality"]["type"], "score");
        assert_eq!(questions["next_action"]["type"], "choice");
    }

    #[test]
    fn bounded_state_keeps_small_values_and_marks_large_values() {
        let small = json!({"query":"auth"});
        assert_eq!(bounded_state(small.clone(), 100), small);
        let large = bounded_state(json!({"text":"x".repeat(100)}), 20);
        assert_eq!(large["truncated"], true);
    }

    #[tokio::test]
    async fn client_sends_bearer_request_and_decodes_typed_response() {
        let app = Router::new().route(
            "/systemone",
            post(|headers: HeaderMap, Json(body): Json<Value>| async move {
                assert_eq!(
                    headers
                        .get("authorization")
                        .and_then(|value| value.to_str().ok()),
                    Some("Bearer test-key")
                );
                assert_eq!(body["model"], DEFAULT_MODEL);
                assert!(body["questions"]["answerable"].is_object());
                (
                    [("content-type", "application/json")],
                    json!({
                        "model": "jev-1.13.0",
                        "answers": {
                            "answerable": {"type":"noul", "noul":0.9}
                        },
                        "usage": {"input_tokens": 10, "output_tokens": 4}
                    })
                    .to_string(),
                )
            }),
        );
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("address");
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve");
        });

        let client = TypeSafeClient::with_endpoint_for_test(
            format!("http://{address}/systemone"),
            "test-key",
        );
        let response = client
            .evaluate(json!({"query":"auth"}), ask_questions())
            .await
            .expect("response");
        assert_eq!(response.model, "jev-1.13.0");
        assert_eq!(response.answers["answerable"]["noul"], 0.9);
        assert_eq!(response.usage.expect("usage").input_tokens, Some(10));
    }
}
