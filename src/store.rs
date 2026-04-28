use reqwest::blocking::{Client, Response};
use reqwest::header::AUTHORIZATION;
use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Clone)]
pub struct ServerApi {
    base_url: String,
    client: Client,
    token: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RoomSummary {
    pub name: String,
    pub owner: String,
    pub limit: usize,
    pub members: usize,
}

#[derive(Clone, Debug, Deserialize)]
pub struct StoredMessage {
    pub id: u64,
    pub room: String,
    pub from: String,
    pub timestamp: u64,
    pub ciphertext: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedSession {
    pub server: String,
    pub user: String,
    pub token: String,
}

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug, Deserialize)]
struct AuthResponse {
    user: String,
    token: String,
}

#[derive(Debug, Serialize)]
struct CredentialsRequest<'a> {
    user: &'a str,
    password: &'a str,
}

#[derive(Debug, Serialize)]
struct CreateRoomRequest<'a> {
    name: &'a str,
    limit: Option<usize>,
}

#[derive(Debug, Serialize)]
struct SetLimitRequest {
    limit: usize,
}

#[derive(Debug, Serialize)]
struct TargetRequest<'a> {
    target: &'a str,
}

#[derive(Debug, Serialize)]
struct SendMessageRequest<'a> {
    ciphertext: &'a str,
}

pub struct SessionStore {
    path: PathBuf,
}

impl SessionStore {
    pub fn new<P: Into<PathBuf>>(path: P) -> Self {
        Self { path: path.into() }
    }

    pub fn load(&self) -> Result<Option<SavedSession>, String> {
        if !self.path.exists() {
            return Ok(None);
        }
        let raw = fs::read_to_string(&self.path)
            .map_err(|error| format!("failed to read {}: {error}", self.path.display()))?;
        let session = serde_json::from_str::<SavedSession>(&raw)
            .map_err(|error| format!("failed to parse {}: {error}", self.path.display()))?;
        Ok(Some(session))
    }

    pub fn save(&self, session: &SavedSession) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
        }
        let raw = serde_json::to_string_pretty(session)
            .map_err(|error| format!("failed to serialize session: {error}"))?;
        fs::write(&self.path, raw)
            .map_err(|error| format!("failed to write {}: {error}", self.path.display()))
    }

    pub fn clear(&self) -> Result<(), String> {
        if self.path.exists() {
            fs::remove_file(&self.path)
                .map_err(|error| format!("failed to remove {}: {error}", self.path.display()))?;
        }
        Ok(())
    }
}

impl ServerApi {
    pub fn new(base_url: impl Into<String>, token: Option<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            client: Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .expect("failed to build HTTP client"),
            token,
        }
    }

    pub fn register(&self, user: &str, password: &str) -> Result<SavedSession, String> {
        let response = self.expect_json::<AuthResponse>(
            self.client
                .post(format!("{}/auth/register", self.base_url))
                .json(&CredentialsRequest { user, password })
                .send()
                .map_err(|error| format!("failed to send register request: {error}"))?,
            &[StatusCode::CREATED],
        )?;
        Ok(SavedSession {
            server: self.base_url.clone(),
            user: response.user,
            token: response.token,
        })
    }

    pub fn login(&self, user: &str, password: &str) -> Result<SavedSession, String> {
        let response = self.expect_json::<AuthResponse>(
            self.client
                .post(format!("{}/auth/login", self.base_url))
                .json(&CredentialsRequest { user, password })
                .send()
                .map_err(|error| format!("failed to send login request: {error}"))?,
            &[StatusCode::OK],
        )?;
        Ok(SavedSession {
            server: self.base_url.clone(),
            user: response.user,
            token: response.token,
        })
    }

    pub fn logout(&self) -> Result<(), String> {
        self.expect_empty(
            self.authorized(self.client.post(format!("{}/auth/logout", self.base_url)))?
                .send()
                .map_err(|error| format!("failed to send logout request: {error}"))?,
            &[StatusCode::OK],
        )
    }

    pub fn create_room(&self, room_name: &str, limit: usize) -> Result<(), String> {
        self.expect_status(
            self.authorized(self.client.post(format!("{}/rooms", self.base_url)))?
                .json(&CreateRoomRequest {
                    name: room_name,
                    limit: Some(limit),
                })
                .send()
                .map_err(|error| format!("failed to create room request: {error}"))?,
            &[StatusCode::CREATED],
        )
        .map(|_| ())
    }

    pub fn join_room(&self, room_name: &str) -> Result<(), String> {
        self.expect_empty(
            self.authorized(self.client.post(format!("{}/rooms/{}/join", self.base_url, room_name)))?
                .send()
                .map_err(|error| format!("failed to join room request: {error}"))?,
            &[StatusCode::OK],
        )
    }

    pub fn leave_room(&self, room_name: &str) -> Result<(), String> {
        self.expect_empty(
            self.authorized(self.client.post(format!("{}/rooms/{}/leave", self.base_url, room_name)))?
                .send()
                .map_err(|error| format!("failed to leave room request: {error}"))?,
            &[StatusCode::OK],
        )
    }

    pub fn set_room_limit(&self, room_name: &str, limit: usize) -> Result<(), String> {
        self.expect_empty(
            self.authorized(self.client.post(format!("{}/rooms/{}/limit", self.base_url, room_name)))?
                .json(&SetLimitRequest { limit })
                .send()
                .map_err(|error| format!("failed to set limit request: {error}"))?,
            &[StatusCode::OK],
        )
    }

    pub fn kick_user(&self, room_name: &str, target: &str) -> Result<(), String> {
        self.expect_empty(
            self.authorized(self.client.post(format!("{}/rooms/{}/kick", self.base_url, room_name)))?
                .json(&TargetRequest { target })
                .send()
                .map_err(|error| format!("failed to kick user request: {error}"))?,
            &[StatusCode::OK],
        )
    }

    pub fn ban_user(&self, room_name: &str, target: &str) -> Result<(), String> {
        self.expect_empty(
            self.authorized(self.client.post(format!("{}/rooms/{}/ban", self.base_url, room_name)))?
                .json(&TargetRequest { target })
                .send()
                .map_err(|error| format!("failed to ban user request: {error}"))?,
            &[StatusCode::OK],
        )
    }

    pub fn list_rooms(&self) -> Result<Vec<RoomSummary>, String> {
        self.expect_json(
            self.authorized(self.client.get(format!("{}/rooms", self.base_url)))?
                .send()
                .map_err(|error| format!("failed to list rooms request: {error}"))?,
            &[StatusCode::OK],
        )
    }

    pub fn list_members(&self, room_name: &str) -> Result<Vec<String>, String> {
        self.expect_json(
            self.authorized(self.client.get(format!("{}/rooms/{}/members", self.base_url, room_name)))?
                .send()
                .map_err(|error| format!("failed to list members request: {error}"))?,
            &[StatusCode::OK],
        )
    }

    pub fn send_message(&self, room_name: &str, ciphertext: &str) -> Result<(), String> {
        self.expect_empty(
            self.authorized(self.client.post(format!("{}/rooms/{}/messages", self.base_url, room_name)))?
                .json(&SendMessageRequest { ciphertext })
                .send()
                .map_err(|error| format!("failed to send message request: {error}"))?,
            &[StatusCode::CREATED],
        )
    }

    pub fn read_messages(&self, room_name: &str, after_id: Option<u64>) -> Result<Vec<StoredMessage>, String> {
        let url = match after_id {
            Some(after_id) => format!("{}/rooms/{}/messages?after_id={after_id}", self.base_url, room_name),
            None => format!("{}/rooms/{}/messages", self.base_url, room_name),
        };
        self.expect_json(
            self.authorized(self.client.get(url))?
                .send()
                .map_err(|error| format!("failed to read messages request: {error}"))?,
            &[StatusCode::OK],
        )
    }

    fn authorized(&self, builder: reqwest::blocking::RequestBuilder) -> Result<reqwest::blocking::RequestBuilder, String> {
        let token = self
            .token
            .as_ref()
            .ok_or_else(|| "not authorized on this PC, login first".to_string())?;
        Ok(builder.header(AUTHORIZATION, format!("Bearer {token}")))
    }

    fn expect_empty(&self, response: Response, allowed: &[StatusCode]) -> Result<(), String> {
        self.expect_status(response, allowed).map(|_| ())
    }

    fn expect_json<T: DeserializeOwned>(&self, response: Response, allowed: &[StatusCode]) -> Result<T, String> {
        let response = self.expect_status(response, allowed)?;
        response
            .json::<T>()
            .map_err(|error| format!("failed to decode server response: {error}"))
    }

    fn expect_status(&self, response: Response, allowed: &[StatusCode]) -> Result<Response, String> {
        let status = response.status();
        if allowed.contains(&status) {
            return Ok(response);
        }

        let body = response.text().unwrap_or_default();
        if body.is_empty() {
            return Err(format!("server returned {}", status));
        }

        if let Ok(parsed) = serde_json::from_str::<ErrorResponse>(&body) {
            return Err(parsed.error);
        }

        Err(format!("server returned {}: {}", status, body.trim()))
    }
}

