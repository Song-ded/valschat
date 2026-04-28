use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::net::SocketAddr;
use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

const MAX_CIPHERTEXT_LEN: usize = 8192;
const AUTH_WINDOW_SECS: u64 = 60;
const AUTH_LIMIT: usize = 12;
const ROOM_WINDOW_SECS: u64 = 60;
const ROOM_LIMIT: usize = 30;
const MESSAGE_WINDOW_SECS: u64 = 10;
const MESSAGE_LIMIT: usize = 12;

#[derive(Clone)]
struct AppState {
    data: Arc<RwLock<ServerData>>,
    rates: Arc<RwLock<RateLimitState>>,
    file_path: PathBuf,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct ServerData {
    next_message_id: u64,
    next_token_id: u64,
    rooms: BTreeMap<String, RoomState>,
    users: BTreeMap<String, UserAccount>,
    sessions: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct UserAccount {
    salt: String,
    password_hash: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct RoomState {
    name: String,
    owner: String,
    limit: usize,
    members: BTreeSet<String>,
    banned: BTreeSet<String>,
    messages: Vec<StoredMessage>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct StoredMessage {
    id: u64,
    room: String,
    from: String,
    timestamp: u64,
    ciphertext: String,
}

#[derive(Default)]
struct RateLimitState {
    entries: BTreeMap<String, Vec<u64>>,
}

#[derive(Debug, Deserialize)]
struct CredentialsRequest {
    user: String,
    password: String,
}

#[derive(Debug, Deserialize)]
struct CreateRoomRequest {
    name: String,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct SetLimitRequest {
    limit: usize,
}

#[derive(Debug, Deserialize)]
struct TargetRequest {
    target: String,
}

#[derive(Debug, Deserialize)]
struct SendMessageRequest {
    ciphertext: String,
}

#[derive(Debug, Deserialize)]
struct MessagesQuery {
    after_id: Option<u64>,
}

#[derive(Debug, Serialize)]
struct RoomSummary {
    name: String,
    owner: String,
    limit: usize,
    members: usize,
}

#[derive(Debug, Serialize)]
struct AuthResponse {
    user: String,
    token: String,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("server error: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), String> {
    let file_path = PathBuf::from("server-data/state.json");
    let data = load_state(&file_path)?;
    let state = AppState {
        data: Arc::new(RwLock::new(data)),
        rates: Arc::new(RwLock::new(RateLimitState::default())),
        file_path,
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/auth/register", post(register))
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
        .route("/rooms", get(list_rooms).post(create_room))
        .route("/rooms/{room}/join", post(join_room))
        .route("/rooms/{room}/leave", post(leave_room))
        .route("/rooms/{room}/limit", post(set_room_limit))
        .route("/rooms/{room}/kick", post(kick_user))
        .route("/rooms/{room}/ban", post(ban_user))
        .route("/rooms/{room}/members", get(list_members))
        .route("/rooms/{room}/messages", get(get_messages).post(send_message))
        .with_state(state);

    let port = std::env::var("PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(25655);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    println!("server listening on http://{addr}");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|error| format!("failed to bind {addr}: {error}"))?;
    axum::serve(listener, app)
        .await
        .map_err(|error| format!("server stopped: {error}"))
}

async fn health() -> &'static str {
    "ok"
}

async fn register(
    State(state): State<AppState>,
    Json(request): Json<CredentialsRequest>,
) -> Result<(StatusCode, Json<AuthResponse>), (StatusCode, Json<ErrorResponse>)> {
    validate_name(&request.user, "user")?;
    validate_password(&request.password)?;
    enforce_rate_limit(&state, &format!("auth:{}", request.user), AUTH_LIMIT, AUTH_WINDOW_SECS).await?;

    let mut data = state.data.write().await;
    if data.users.contains_key(&request.user) {
        return Err(error(StatusCode::CONFLICT, "user already exists"));
    }

    let salt = make_token_seed(&request.user, data.next_token_id);
    data.next_token_id += 1;
    let password_hash = hash_password(&request.user, &request.password, &salt);
    data.users.insert(
        request.user.clone(),
        UserAccount {
            salt,
            password_hash,
        },
    );

    let token = issue_token(&mut data, &request.user)?;
    save_state(&state.file_path, &data)?;
    Ok((
        StatusCode::CREATED,
        Json(AuthResponse {
            user: request.user,
            token,
        }),
    ))
}

async fn login(
    State(state): State<AppState>,
    Json(request): Json<CredentialsRequest>,
) -> Result<Json<AuthResponse>, (StatusCode, Json<ErrorResponse>)> {
    validate_name(&request.user, "user")?;
    validate_password(&request.password)?;
    enforce_rate_limit(&state, &format!("auth:{}", request.user), AUTH_LIMIT, AUTH_WINDOW_SECS).await?;

    let mut data = state.data.write().await;
    let account = data
        .users
        .get(&request.user)
        .ok_or_else(|| error(StatusCode::FORBIDDEN, "invalid credentials"))?;
    let expected = hash_password(&request.user, &request.password, &account.salt);
    if expected != account.password_hash {
        return Err(error(StatusCode::FORBIDDEN, "invalid credentials"));
    }

    let token = issue_token(&mut data, &request.user)?;
    save_state(&state.file_path, &data)?;
    Ok(Json(AuthResponse {
        user: request.user,
        token,
    }))
}

async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let token = bearer_token(&headers)?;
    let mut data = state.data.write().await;
    data.sessions.remove(&token);
    save_state(&state.file_path, &data)?;
    Ok(StatusCode::OK)
}

async fn list_rooms(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<RoomSummary>>, (StatusCode, Json<ErrorResponse>)> {
    let _user = authenticated_user(&state, &headers).await?;
    let data = state.data.read().await;
    let rooms = data
        .rooms
        .values()
        .map(|room| RoomSummary {
            name: room.name.clone(),
            owner: room.owner.clone(),
            limit: room.limit,
            members: room.members.len(),
        })
        .collect();
    Ok(Json(rooms))
}

async fn create_room(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateRoomRequest>,
) -> Result<(StatusCode, Json<RoomSummary>), (StatusCode, Json<ErrorResponse>)> {
    validate_name(&request.name, "room")?;
    let owner = authenticated_user(&state, &headers).await?;
    enforce_rate_limit(&state, &format!("room:{owner}"), ROOM_LIMIT, ROOM_WINDOW_SECS).await?;
    let limit = request.limit.unwrap_or(25);
    if limit == 0 {
        return Err(error(StatusCode::BAD_REQUEST, "room limit must be greater than zero"));
    }

    let mut data = state.data.write().await;
    if data.rooms.contains_key(&request.name) {
        return Err(error(StatusCode::CONFLICT, "room already exists"));
    }

    let mut members = BTreeSet::new();
    members.insert(owner.clone());
    data.rooms.insert(
        request.name.clone(),
        RoomState {
            name: request.name.clone(),
            owner: owner.clone(),
            limit,
            members,
            banned: BTreeSet::new(),
            messages: Vec::new(),
        },
    );
    save_state(&state.file_path, &data)?;

    Ok((
        StatusCode::CREATED,
        Json(RoomSummary {
            name: request.name,
            owner,
            limit,
            members: 1,
        }),
    ))
}

async fn join_room(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(room_name): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let user = authenticated_user(&state, &headers).await?;
    enforce_rate_limit(&state, &format!("room:{user}"), ROOM_LIMIT, ROOM_WINDOW_SECS).await?;

    let mut data = state.data.write().await;
    let room = data
        .rooms
        .get_mut(&room_name)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "room not found"))?;

    if room.banned.contains(&user) {
        return Err(error(StatusCode::FORBIDDEN, "user is banned from this room"));
    }
    if room.members.contains(&user) {
        return Ok(StatusCode::OK);
    }
    if room.members.len() >= room.limit {
        return Err(error(StatusCode::CONFLICT, "room is full"));
    }

    room.members.insert(user);
    save_state(&state.file_path, &data)?;
    Ok(StatusCode::OK)
}

async fn leave_room(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(room_name): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let user = authenticated_user(&state, &headers).await?;
    enforce_rate_limit(&state, &format!("room:{user}"), ROOM_LIMIT, ROOM_WINDOW_SECS).await?;

    let mut data = state.data.write().await;
    let room = data
        .rooms
        .get_mut(&room_name)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "room not found"))?;

    if !room.members.remove(&user) {
        return Err(error(StatusCode::NOT_FOUND, "user is not in this room"));
    }
    if room.owner == user && !room.members.is_empty() {
        room.members.insert(user);
        return Err(error(StatusCode::CONFLICT, "owner cannot leave while other users are still in the room"));
    }

    cleanup_empty_room(&mut data, &room_name);
    save_state(&state.file_path, &data)?;
    Ok(StatusCode::OK)
}

async fn set_room_limit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(room_name): Path<String>,
    Json(request): Json<SetLimitRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let user = authenticated_user(&state, &headers).await?;
    enforce_rate_limit(&state, &format!("room:{user}"), ROOM_LIMIT, ROOM_WINDOW_SECS).await?;
    if request.limit == 0 {
        return Err(error(StatusCode::BAD_REQUEST, "room limit must be greater than zero"));
    }

    let mut data = state.data.write().await;
    let room = data
        .rooms
        .get_mut(&room_name)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "room not found"))?;
    if room.owner != user {
        return Err(error(StatusCode::FORBIDDEN, "only the room owner can change the limit"));
    }
    if request.limit < room.members.len() {
        return Err(error(StatusCode::CONFLICT, "new limit is smaller than current room size"));
    }

    room.limit = request.limit;
    save_state(&state.file_path, &data)?;
    Ok(StatusCode::OK)
}

async fn kick_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(room_name): Path<String>,
    Json(request): Json<TargetRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    validate_name(&request.target, "user")?;
    let user = authenticated_user(&state, &headers).await?;
    enforce_rate_limit(&state, &format!("room:{user}"), ROOM_LIMIT, ROOM_WINDOW_SECS).await?;

    let mut data = state.data.write().await;
    let room = data
        .rooms
        .get_mut(&room_name)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "room not found"))?;
    if room.owner != user {
        return Err(error(StatusCode::FORBIDDEN, "only the room owner can kick users"));
    }
    if room.owner == request.target {
        return Err(error(StatusCode::CONFLICT, "owner cannot kick themselves"));
    }
    if !room.members.remove(&request.target) {
        return Err(error(StatusCode::NOT_FOUND, "user is not in this room"));
    }

    cleanup_empty_room(&mut data, &room_name);
    save_state(&state.file_path, &data)?;
    Ok(StatusCode::OK)
}

async fn ban_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(room_name): Path<String>,
    Json(request): Json<TargetRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    validate_name(&request.target, "user")?;
    let user = authenticated_user(&state, &headers).await?;
    enforce_rate_limit(&state, &format!("room:{user}"), ROOM_LIMIT, ROOM_WINDOW_SECS).await?;

    let mut data = state.data.write().await;
    let room = data
        .rooms
        .get_mut(&room_name)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "room not found"))?;
    if room.owner != user {
        return Err(error(StatusCode::FORBIDDEN, "only the room owner can ban users"));
    }
    if room.owner == request.target {
        return Err(error(StatusCode::CONFLICT, "owner cannot ban themselves"));
    }

    room.banned.insert(request.target.clone());
    room.members.remove(&request.target);
    cleanup_empty_room(&mut data, &room_name);
    save_state(&state.file_path, &data)?;
    Ok(StatusCode::OK)
}

async fn list_members(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(room_name): Path<String>,
) -> Result<Json<Vec<String>>, (StatusCode, Json<ErrorResponse>)> {
    let _user = authenticated_user(&state, &headers).await?;
    let data = state.data.read().await;
    let room = data
        .rooms
        .get(&room_name)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "room not found"))?;
    Ok(Json(room.members.iter().cloned().collect()))
}

async fn send_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(room_name): Path<String>,
    Json(request): Json<SendMessageRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let user = authenticated_user(&state, &headers).await?;
    enforce_rate_limit(&state, &format!("message:{user}"), MESSAGE_LIMIT, MESSAGE_WINDOW_SECS).await?;
    if request.ciphertext.trim().is_empty() {
        return Err(error(StatusCode::BAD_REQUEST, "ciphertext must not be empty"));
    }
    if request.ciphertext.len() > MAX_CIPHERTEXT_LEN {
        return Err(error(StatusCode::BAD_REQUEST, "ciphertext is too large"));
    }

    let mut data = state.data.write().await;
    let message_id = data.next_message_id.max(1);
    data.next_message_id = message_id + 1;
    let room = data
        .rooms
        .get_mut(&room_name)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "room not found"))?;
    if !room.members.contains(&user) {
        return Err(error(StatusCode::FORBIDDEN, "user is not in this room"));
    }

    room.messages.push(StoredMessage {
        id: message_id,
        room: room_name.clone(),
        from: user,
        timestamp: unix_time_now()?,
        ciphertext: request.ciphertext,
    });
    save_state(&state.file_path, &data)?;
    Ok(StatusCode::CREATED)
}

async fn get_messages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(room_name): Path<String>,
    Query(query): Query<MessagesQuery>,
) -> Result<Json<Vec<StoredMessage>>, (StatusCode, Json<ErrorResponse>)> {
    let user = authenticated_user(&state, &headers).await?;
    let data = state.data.read().await;
    let room = data
        .rooms
        .get(&room_name)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "room not found"))?;
    if !room.members.contains(&user) {
        return Err(error(StatusCode::FORBIDDEN, "user is not in this room"));
    }

    let messages = room
        .messages
        .iter()
        .filter(|message| query.after_id.is_none_or(|after_id| message.id > after_id))
        .cloned()
        .collect();
    Ok(Json(messages))
}

async fn authenticated_user(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<String, (StatusCode, Json<ErrorResponse>)> {
    let token = bearer_token(headers)?;
    let data = state.data.read().await;
    data.sessions
        .get(&token)
        .cloned()
        .ok_or_else(|| error(StatusCode::UNAUTHORIZED, "invalid session"))
}

fn bearer_token(headers: &HeaderMap) -> Result<String, (StatusCode, Json<ErrorResponse>)> {
    let value = headers
        .get("authorization")
        .ok_or_else(|| error(StatusCode::UNAUTHORIZED, "missing authorization"))?
        .to_str()
        .map_err(|_| error(StatusCode::UNAUTHORIZED, "invalid authorization header"))?;
    let token = value
        .strip_prefix("Bearer ")
        .ok_or_else(|| error(StatusCode::UNAUTHORIZED, "missing bearer token"))?;
    if token.trim().is_empty() {
        return Err(error(StatusCode::UNAUTHORIZED, "missing bearer token"));
    }
    Ok(token.to_string())
}

async fn enforce_rate_limit(
    state: &AppState,
    key: &str,
    limit: usize,
    window_secs: u64,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    let now = unix_time_now()?;
    let mut rates = state.rates.write().await;
    let bucket = rates.entries.entry(key.to_string()).or_default();
    bucket.retain(|timestamp| now.saturating_sub(*timestamp) < window_secs);
    if bucket.len() >= limit {
        return Err(error(StatusCode::TOO_MANY_REQUESTS, "rate limit exceeded"));
    }
    bucket.push(now);
    Ok(())
}

fn cleanup_empty_room(data: &mut ServerData, room_name: &str) {
    if data.rooms.get(room_name).is_some_and(|room| !room.members.is_empty()) {
        return;
    }
    data.rooms.remove(room_name);
}

fn validate_name(value: &str, kind: &str) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    if value.trim().is_empty() {
        return Err(error(StatusCode::BAD_REQUEST, &format!("{kind} name must not be empty")));
    }
    if value.contains('\n') || value.contains('\r') || value.contains('\t') {
        return Err(error(StatusCode::BAD_REQUEST, &format!("{kind} name must not contain tabs or newlines")));
    }
    Ok(())
}

fn validate_password(password: &str) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    if password.len() < 4 {
        return Err(error(StatusCode::BAD_REQUEST, "password must be at least 4 characters"));
    }
    if password.contains('\n') || password.contains('\r') || password.contains('\t') {
        return Err(error(StatusCode::BAD_REQUEST, "password must not contain tabs or newlines"));
    }
    Ok(())
}

fn issue_token(
    data: &mut ServerData,
    user: &str,
) -> Result<String, (StatusCode, Json<ErrorResponse>)> {
    let seed = make_token_seed(user, data.next_token_id);
    data.next_token_id += 1;
    let token = mix_hex(&[seed.as_bytes(), user.as_bytes(), &data.next_token_id.to_le_bytes()]);
    data.sessions.insert(token.clone(), user.to_string());
    Ok(token)
}

fn make_token_seed(user: &str, counter: u64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("{user}:{counter}:{now}")
}

fn hash_password(user: &str, password: &str, salt: &str) -> String {
    let mut current = mix_bytes(&[user.as_bytes(), password.as_bytes(), salt.as_bytes()]);
    for round in 0..2048u64 {
        current = mix_bytes(&[&current, salt.as_bytes(), &round.to_le_bytes()]);
    }
    hex_encode(&current)
}

fn mix_hex(parts: &[&[u8]]) -> String {
    hex_encode(&mix_bytes(parts))
}

fn mix_bytes(parts: &[&[u8]]) -> [u8; 16] {
    let mut left = 0xcbf2_9ce4_8422_2325u64;
    let mut right = 0x9e37_79b9_7f4a_7c15u64;
    for part in parts {
        for byte in *part {
            left ^= *byte as u64;
            left = left.wrapping_mul(0x1000_0000_01b3).rotate_left(7);
            right ^= ((*byte as u64) << 1) | 1;
            right = right.wrapping_mul(0x9ddf_ea08_eb38_2d69).rotate_left(11);
        }
        left ^= part.len() as u64;
        right ^= (part.len() as u64).rotate_left(13);
    }
    let mut output = [0u8; 16];
    output[..8].copy_from_slice(&left.to_le_bytes());
    output[8..].copy_from_slice(&right.to_le_bytes());
    output
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(b"0123456789abcdef"[(byte >> 4) as usize]));
        output.push(char::from(b"0123456789abcdef"[(byte & 0x0f) as usize]));
    }
    output
}

fn error(status: StatusCode, message: &str) -> (StatusCode, Json<ErrorResponse>) {
    (status, Json(ErrorResponse { error: message.to_string() }))
}

fn load_state(path: &FsPath) -> Result<ServerData, String> {
    if !path.exists() {
        return Ok(ServerData {
            next_message_id: 1,
            next_token_id: 1,
            rooms: BTreeMap::new(),
            users: BTreeMap::new(),
            sessions: BTreeMap::new(),
        });
    }

    let raw = fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    serde_json::from_str(&raw)
        .map_err(|error| format!("failed to parse {}: {error}", path.display()))
}

fn save_state(path: &FsPath, data: &ServerData) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|io_error| error(StatusCode::INTERNAL_SERVER_ERROR, &format!("failed to create {}: {io_error}", parent.display())))?;
    }

    let raw = serde_json::to_string_pretty(data)
        .map_err(|serde_error| error(StatusCode::INTERNAL_SERVER_ERROR, &format!("failed to serialize server state: {serde_error}")))?;
    fs::write(path, raw)
        .map_err(|io_error| error(StatusCode::INTERNAL_SERVER_ERROR, &format!("failed to write {}: {io_error}", path.display())))
}

fn unix_time_now() -> Result<u64, (StatusCode, Json<ErrorResponse>)> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "system clock is before unix epoch"))
}

