use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
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

#[derive(Clone)]
struct AppState {
    data: Arc<RwLock<ServerData>>,
    file_path: PathBuf,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct ServerData {
    next_message_id: u64,
    rooms: BTreeMap<String, RoomState>,
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

#[derive(Debug, Deserialize)]
struct CreateRoomRequest {
    name: String,
    owner: String,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct UserRequest {
    user: String,
}

#[derive(Debug, Deserialize)]
struct SetLimitRequest {
    owner: String,
    limit: usize,
}

#[derive(Debug, Deserialize)]
struct OwnerTargetRequest {
    owner: String,
    target: String,
}

#[derive(Debug, Deserialize)]
struct SendMessageRequest {
    from: String,
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
        file_path,
    };

    let app = Router::new()
        .route("/health", get(health))
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
        .unwrap_or(8080);
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

async fn list_rooms(State(state): State<AppState>) -> Json<Vec<RoomSummary>> {
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
    Json(rooms)
}

async fn create_room(
    State(state): State<AppState>,
    Json(request): Json<CreateRoomRequest>,
) -> Result<(StatusCode, Json<RoomSummary>), (StatusCode, Json<ErrorResponse>)> {
    validate_name(&request.name, "room")?;
    validate_name(&request.owner, "user")?;
    let limit = request.limit.unwrap_or(25);
    if limit == 0 {
        return Err(error(StatusCode::BAD_REQUEST, "room limit must be greater than zero"));
    }

    let mut data = state.data.write().await;
    if data.rooms.contains_key(&request.name) {
        return Err(error(StatusCode::CONFLICT, "room already exists"));
    }

    let mut members = BTreeSet::new();
    members.insert(request.owner.clone());
    data.rooms.insert(
        request.name.clone(),
        RoomState {
            name: request.name.clone(),
            owner: request.owner.clone(),
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
            owner: request.owner,
            limit,
            members: 1,
        }),
    ))
}

async fn join_room(
    State(state): State<AppState>,
    Path(room_name): Path<String>,
    Json(request): Json<UserRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    validate_name(&request.user, "user")?;
    let mut data = state.data.write().await;
    let room = data
        .rooms
        .get_mut(&room_name)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "room not found"))?;

    if room.banned.contains(&request.user) {
        return Err(error(StatusCode::FORBIDDEN, "user is banned from this room"));
    }
    if room.members.contains(&request.user) {
        return Ok(StatusCode::OK);
    }
    if room.members.len() >= room.limit {
        return Err(error(StatusCode::CONFLICT, "room is full"));
    }

    room.members.insert(request.user);
    save_state(&state.file_path, &data)?;
    Ok(StatusCode::OK)
}

async fn leave_room(
    State(state): State<AppState>,
    Path(room_name): Path<String>,
    Json(request): Json<UserRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    validate_name(&request.user, "user")?;
    let mut data = state.data.write().await;
    let room = data
        .rooms
        .get_mut(&room_name)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "room not found"))?;

    if !room.members.remove(&request.user) {
        return Err(error(StatusCode::NOT_FOUND, "user is not in this room"));
    }

    if room.owner == request.user && !room.members.is_empty() {
        room.members.insert(request.user);
        return Err(error(
            StatusCode::CONFLICT,
            "owner cannot leave while other users are still in the room",
        ));
    }

    cleanup_empty_room(&mut data, &room_name);
    save_state(&state.file_path, &data)?;
    Ok(StatusCode::OK)
}

async fn set_room_limit(
    State(state): State<AppState>,
    Path(room_name): Path<String>,
    Json(request): Json<SetLimitRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    validate_name(&request.owner, "user")?;
    if request.limit == 0 {
        return Err(error(StatusCode::BAD_REQUEST, "room limit must be greater than zero"));
    }

    let mut data = state.data.write().await;
    let room = data
        .rooms
        .get_mut(&room_name)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "room not found"))?;
    if room.owner != request.owner {
        return Err(error(StatusCode::FORBIDDEN, "only the room owner can change the limit"));
    }
    if request.limit < room.members.len() {
        return Err(error(
            StatusCode::CONFLICT,
            "new limit is smaller than current room size",
        ));
    }

    room.limit = request.limit;
    save_state(&state.file_path, &data)?;
    Ok(StatusCode::OK)
}

async fn kick_user(
    State(state): State<AppState>,
    Path(room_name): Path<String>,
    Json(request): Json<OwnerTargetRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    validate_name(&request.owner, "user")?;
    validate_name(&request.target, "user")?;
    let mut data = state.data.write().await;
    let room = data
        .rooms
        .get_mut(&room_name)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "room not found"))?;
    if room.owner != request.owner {
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
    Path(room_name): Path<String>,
    Json(request): Json<OwnerTargetRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    validate_name(&request.owner, "user")?;
    validate_name(&request.target, "user")?;
    let mut data = state.data.write().await;
    let room = data
        .rooms
        .get_mut(&room_name)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "room not found"))?;
    if room.owner != request.owner {
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
    Path(room_name): Path<String>,
) -> Result<Json<Vec<String>>, (StatusCode, Json<ErrorResponse>)> {
    let data = state.data.read().await;
    let room = data
        .rooms
        .get(&room_name)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "room not found"))?;
    Ok(Json(room.members.iter().cloned().collect()))
}

async fn send_message(
    State(state): State<AppState>,
    Path(room_name): Path<String>,
    Json(request): Json<SendMessageRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    validate_name(&request.from, "user")?;
    if request.ciphertext.trim().is_empty() {
        return Err(error(StatusCode::BAD_REQUEST, "ciphertext must not be empty"));
    }

    let mut data = state.data.write().await;
    let message_id = data.next_message_id;
    data.next_message_id += 1;
    let room = data
        .rooms
        .get_mut(&room_name)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "room not found"))?;
    if !room.members.contains(&request.from) {
        return Err(error(StatusCode::FORBIDDEN, "user is not in this room"));
    }

    let message = StoredMessage {
        id: message_id,
        room: room_name.clone(),
        from: request.from,
        timestamp: unix_time_now()?,
        ciphertext: request.ciphertext,
    };
    room.messages.push(message);
    save_state(&state.file_path, &data)?;
    Ok(StatusCode::CREATED)
}

async fn get_messages(
    State(state): State<AppState>,
    Path(room_name): Path<String>,
    Query(query): Query<MessagesQuery>,
) -> Result<Json<Vec<StoredMessage>>, (StatusCode, Json<ErrorResponse>)> {
    let data = state.data.read().await;
    let room = data
        .rooms
        .get(&room_name)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "room not found"))?;

    let messages = room
        .messages
        .iter()
        .filter(|message| query.after_id.is_none_or(|after_id| message.id > after_id))
        .cloned()
        .collect();
    Ok(Json(messages))
}

fn cleanup_empty_room(data: &mut ServerData, room_name: &str) {
    if data
        .rooms
        .get(room_name)
        .is_some_and(|room| !room.members.is_empty())
    {
        return;
    }
    data.rooms.remove(room_name);
}

fn validate_name(value: &str, kind: &str) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    if value.trim().is_empty() {
        return Err(error(
            StatusCode::BAD_REQUEST,
            &format!("{kind} name must not be empty"),
        ));
    }
    if value.contains('\n') || value.contains('\r') || value.contains('\t') {
        return Err(error(
            StatusCode::BAD_REQUEST,
            &format!("{kind} name must not contain tabs or newlines"),
        ));
    }
    Ok(())
}

fn error(status: StatusCode, message: &str) -> (StatusCode, Json<ErrorResponse>) {
    (
        status,
        Json(ErrorResponse {
            error: message.to_string(),
        }),
    )
}

fn load_state(path: &FsPath) -> Result<ServerData, String> {
    if !path.exists() {
        return Ok(ServerData {
            next_message_id: 1,
            rooms: BTreeMap::new(),
        });
    }

    let raw = fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    serde_json::from_str(&raw)
        .map_err(|error| format!("failed to parse {}: {error}", path.display()))
}

fn save_state(path: &FsPath, data: &ServerData) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|io_error| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("failed to create {}: {io_error}", parent.display()),
            )
        })?;
    }

    let raw = serde_json::to_string_pretty(data).map_err(|serde_error| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("failed to serialize server state: {serde_error}"),
        )
    })?;
    fs::write(path, raw).map_err(|io_error| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("failed to write {}: {io_error}", path.display()),
        )
    })
}

fn unix_time_now() -> Result<u64, (StatusCode, Json<ErrorResponse>)> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "system clock is before unix epoch"))
}
