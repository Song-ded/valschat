use reqwest::blocking::{Client, Response};
use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Clone)]
pub struct ServerApi {
    base_url: String,
    client: Client,
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

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug, Serialize)]
struct CreateRoomRequest<'a> {
    name: &'a str,
    owner: &'a str,
    limit: Option<usize>,
}

#[derive(Debug, Serialize)]
struct UserRequest<'a> {
    user: &'a str,
}

#[derive(Debug, Serialize)]
struct SetLimitRequest<'a> {
    owner: &'a str,
    limit: usize,
}

#[derive(Debug, Serialize)]
struct OwnerTargetRequest<'a> {
    owner: &'a str,
    target: &'a str,
}

#[derive(Debug, Serialize)]
struct SendMessageRequest<'a> {
    from: &'a str,
    ciphertext: &'a str,
}

impl ServerApi {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            client: Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .expect("failed to build HTTP client"),
        }
    }

    pub fn create_room(&self, owner: &str, room_name: &str, limit: usize) -> Result<(), String> {
        self.expect_status(
            self.client
                .post(format!("{}/rooms", self.base_url))
                .json(&CreateRoomRequest {
                    name: room_name,
                    owner,
                    limit: Some(limit),
                })
                .send()
                .map_err(|error| format!("failed to create room request: {error}"))?,
            &[StatusCode::CREATED],
        )
        .map(|_| ())
    }

    pub fn join_room(&self, user: &str, room_name: &str) -> Result<(), String> {
        self.expect_empty(
            self.client
                .post(format!("{}/rooms/{}/join", self.base_url, room_name))
                .json(&UserRequest { user })
                .send()
                .map_err(|error| format!("failed to join room request: {error}"))?,
            &[StatusCode::OK],
        )
    }

    pub fn leave_room(&self, user: &str, room_name: &str) -> Result<(), String> {
        self.expect_empty(
            self.client
                .post(format!("{}/rooms/{}/leave", self.base_url, room_name))
                .json(&UserRequest { user })
                .send()
                .map_err(|error| format!("failed to leave room request: {error}"))?,
            &[StatusCode::OK],
        )
    }

    pub fn set_room_limit(&self, owner: &str, room_name: &str, limit: usize) -> Result<(), String> {
        self.expect_empty(
            self.client
                .post(format!("{}/rooms/{}/limit", self.base_url, room_name))
                .json(&SetLimitRequest { owner, limit })
                .send()
                .map_err(|error| format!("failed to set limit request: {error}"))?,
            &[StatusCode::OK],
        )
    }

    pub fn kick_user(&self, owner: &str, room_name: &str, target: &str) -> Result<(), String> {
        self.expect_empty(
            self.client
                .post(format!("{}/rooms/{}/kick", self.base_url, room_name))
                .json(&OwnerTargetRequest { owner, target })
                .send()
                .map_err(|error| format!("failed to kick user request: {error}"))?,
            &[StatusCode::OK],
        )
    }

    pub fn ban_user(&self, owner: &str, room_name: &str, target: &str) -> Result<(), String> {
        self.expect_empty(
            self.client
                .post(format!("{}/rooms/{}/ban", self.base_url, room_name))
                .json(&OwnerTargetRequest { owner, target })
                .send()
                .map_err(|error| format!("failed to ban user request: {error}"))?,
            &[StatusCode::OK],
        )
    }

    pub fn list_rooms(&self) -> Result<Vec<RoomSummary>, String> {
        self.expect_json(
            self.client
                .get(format!("{}/rooms", self.base_url))
                .send()
                .map_err(|error| format!("failed to list rooms request: {error}"))?,
            &[StatusCode::OK],
        )
    }

    pub fn list_members(&self, room_name: &str) -> Result<Vec<String>, String> {
        self.expect_json(
            self.client
                .get(format!("{}/rooms/{}/members", self.base_url, room_name))
                .send()
                .map_err(|error| format!("failed to list members request: {error}"))?,
            &[StatusCode::OK],
        )
    }

    pub fn send_message(&self, room_name: &str, from: &str, ciphertext: &str) -> Result<(), String> {
        self.expect_empty(
            self.client
                .post(format!("{}/rooms/{}/messages", self.base_url, room_name))
                .json(&SendMessageRequest { from, ciphertext })
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
            self.client
                .get(url)
                .send()
                .map_err(|error| format!("failed to read messages request: {error}"))?,
            &[StatusCode::OK],
        )
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
