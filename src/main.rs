mod app;
mod cli;
mod crypto;
mod store;

use app::{ChatMessageView, MessengerApp, RoomView};
use chrono::{DateTime, Local};
use cli::{parse_args, CliCommand};
use crossterm::cursor::MoveToColumn;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::terminal::{self, Clear, ClearType};
use crossterm::{execute, queue};
use crypto::demo_cipher::DemoCipher;
use std::io::{self, Write};
use std::process::ExitCode;
use std::time::{Duration, Instant};
use store::{SavedSession, ServerApi, SessionStore};

const SESSION_PATH: &str = "client-data/session.json";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let command = parse_args(std::env::args().skip(1))?;
    let sessions = SessionStore::new(SESSION_PATH);

    match command {
        CliCommand::Help => {
            cli::print_help();
            Ok(())
        }
        CliCommand::Register { user, password, server } => {
            let session = ServerApi::new(server, None).register(&user, &password)?;
            sessions.save(&session)?;
            println!("registered and logged in as {}", session.user);
            Ok(())
        }
        CliCommand::Login { user, password, server } => {
            let session = ServerApi::new(server, None).login(&user, &password)?;
            sessions.save(&session)?;
            println!("logged in as {}", session.user);
            Ok(())
        }
        CliCommand::Logout { server } => logout(&sessions, server.as_deref()),
        CliCommand::Status => {
            match sessions.load()? {
                Some(session) => {
                    println!("authorized as {}", session.user);
                    println!("server: {}", session.server);
                }
                None => println!("not authorized on this PC"),
            }
            Ok(())
        }
        CliCommand::Chat { server } => {
            let session = sessions
                .load()?
                .ok_or_else(|| "not authorized on this PC, use register or login first".to_string())?;
            let server_url = server.unwrap_or_else(|| session.server.clone());
            let app = MessengerApp::new(
                ServerApi::new(server_url, Some(session.token.clone())),
                DemoCipher::new(),
            );
            run_chat(&app, &session, &sessions)
        }
    }
}

fn logout(sessions: &SessionStore, server_override: Option<&str>) -> Result<(), String> {
    let Some(session) = sessions.load()? else {
        println!("not authorized on this PC");
        return Ok(());
    };

    let server_url = server_override.unwrap_or(&session.server).to_string();
    let api = ServerApi::new(server_url, Some(session.token));
    let _ = api.logout();
    sessions.clear()?;
    println!("logged out on this PC");
    Ok(())
}

fn run_chat(app: &MessengerApp<DemoCipher>, session: &SavedSession, sessions: &SessionStore) -> Result<(), String> {
    let mut current_key = app::DEFAULT_CHAT_KEY.to_string();
    let mut current_room: Option<String> = None;
    let mut current_input = String::new();
    let mut last_seen_id: Option<u64> = None;
    let refresh_interval = Duration::from_millis(500);
    let mut last_refresh = Instant::now();
    let _raw_mode = RawModeGuard::enable()?;

    println!("chat started for {}", session.user);
    println!("active key: {current_key}");
    println!("active room: none");
    println!("use --help for chat commands");
    redraw_prompt(&session.user, current_room.as_deref(), &current_input)?;

    loop {
        if current_room.is_some() && last_refresh.elapsed() >= refresh_interval {
            refresh_new_messages(
                app,
                current_room.as_deref(),
                &current_key,
                &mut last_seen_id,
                &session.user,
                &current_input,
            )?;
            last_refresh = Instant::now();
        }

        if !event::poll(Duration::from_millis(100))
            .map_err(|error| format!("failed to poll terminal events: {error}"))?
        {
            continue;
        }

        let terminal_event =
            event::read().map_err(|error| format!("failed to read terminal event: {error}"))?;

        match terminal_event {
            Event::Key(key_event) => {
                if handle_key_event(
                    key_event,
                    app,
                    &session.user,
                    sessions,
                    &mut current_key,
                    &mut current_room,
                    &mut current_input,
                    &mut last_seen_id,
                )? {
                    println!();
                    return Ok(());
                }
                last_refresh = Instant::now();
            }
            Event::Resize(_, _) => redraw_prompt(&session.user, current_room.as_deref(), &current_input)?,
            _ => {}
        }
    }
}

fn handle_key_event(
    key_event: KeyEvent,
    app: &MessengerApp<DemoCipher>,
    user: &str,
    sessions: &SessionStore,
    current_key: &mut String,
    current_room: &mut Option<String>,
    current_input: &mut String,
    last_seen_id: &mut Option<u64>,
) -> Result<bool, String> {
    if !matches!(key_event.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
        return Ok(false);
    }

    match key_event.code {
        KeyCode::Char('c') if key_event.modifiers.contains(KeyModifiers::CONTROL) => return Ok(true),
        KeyCode::Enter => {
            println!();
            let input = current_input.trim().to_string();
            current_input.clear();

            if input.is_empty() {
                redraw_prompt(user, current_room.as_deref(), current_input)?;
                return Ok(false);
            }

            if input == "--quit" {
                return Ok(true);
            }
            if input == "--help" {
                clear_input_line()?;
                cli::print_chat_help();
                redraw_prompt(user, current_room.as_deref(), current_input)?;
                return Ok(false);
            }
            if input == "--logout" {
                sessions.clear()?;
                println!("logged out on this PC");
                return Ok(true);
            }
            if input == "--refresh" {
                redraw_current_room(app, current_room.as_deref(), current_key, last_seen_id)?;
                redraw_prompt(user, current_room.as_deref(), current_input)?;
                return Ok(false);
            }
            if input == "--rooms" {
                clear_input_line()?;
                print_rooms(app.list_rooms()?);
                redraw_prompt(user, current_room.as_deref(), current_input)?;
                return Ok(false);
            }
            if input == "--members" {
                let room_name = current_room.as_deref().ok_or_else(|| "no active room".to_string())?;
                clear_input_line()?;
                print_members(room_name, app.list_members(room_name)?);
                redraw_prompt(user, current_room.as_deref(), current_input)?;
                return Ok(false);
            }
            if input == "--leave-room" {
                let room_name = current_room.clone().ok_or_else(|| "no active room".to_string())?;
                app.leave_room(&room_name)?;
                *current_room = None;
                *last_seen_id = None;
                clear_input_line()?;
                println!("left room: {room_name}");
                redraw_prompt(user, current_room.as_deref(), current_input)?;
                return Ok(false);
            }
            if let Some(new_key) = input.strip_prefix("--key ") {
                let new_key = new_key.trim();
                if new_key.is_empty() {
                    println!("key must not be empty");
                    redraw_prompt(user, current_room.as_deref(), current_input)?;
                    return Ok(false);
                }
                *current_key = new_key.to_string();
                println!("active key changed to: {current_key}");
                redraw_current_room(app, current_room.as_deref(), current_key, last_seen_id)?;
                redraw_prompt(user, current_room.as_deref(), current_input)?;
                return Ok(false);
            }
            if let Some(args) = input.strip_prefix("--create-room ") {
                let (room_name, limit) = parse_room_creation_args(args)?;
                app.create_room(&room_name, limit)?;
                *current_room = Some(room_name.clone());
                *last_seen_id = None;
                clear_input_line()?;
                println!("created room: {room_name} (limit {limit})");
                redraw_current_room(app, current_room.as_deref(), current_key, last_seen_id)?;
                redraw_prompt(user, current_room.as_deref(), current_input)?;
                return Ok(false);
            }
            if let Some(room_name) = input.strip_prefix("--join-room ") {
                let room_name = room_name.trim();
                if room_name.is_empty() {
                    println!("room name must not be empty");
                    redraw_prompt(user, current_room.as_deref(), current_input)?;
                    return Ok(false);
                }
                app.join_room(room_name)?;
                *current_room = Some(room_name.to_string());
                *last_seen_id = None;
                clear_input_line()?;
                println!("joined room: {room_name}");
                redraw_current_room(app, current_room.as_deref(), current_key, last_seen_id)?;
                redraw_prompt(user, current_room.as_deref(), current_input)?;
                return Ok(false);
            }
            if let Some(limit) = input.strip_prefix("--set-limit ") {
                let room_name = current_room.as_deref().ok_or_else(|| "no active room".to_string())?;
                let limit = limit.trim().parse::<usize>().map_err(|_| "invalid limit".to_string())?;
                app.set_room_limit(room_name, limit)?;
                println!("room limit changed to {limit}");
                redraw_prompt(user, current_room.as_deref(), current_input)?;
                return Ok(false);
            }
            if let Some(target) = input.strip_prefix("--kick ") {
                let room_name = current_room.as_deref().ok_or_else(|| "no active room".to_string())?;
                let target = target.trim();
                app.kick_user(room_name, target)?;
                println!("kicked user: {target}");
                redraw_prompt(user, current_room.as_deref(), current_input)?;
                return Ok(false);
            }
            if let Some(target) = input.strip_prefix("--ban ") {
                let room_name = current_room.as_deref().ok_or_else(|| "no active room".to_string())?;
                let target = target.trim();
                app.ban_user(room_name, target)?;
                println!("banned user: {target}");
                redraw_prompt(user, current_room.as_deref(), current_input)?;
                return Ok(false);
            }

            let room_name = current_room.as_deref().ok_or_else(|| "join a room first".to_string())?;
            app.send_message(room_name, current_key, &input)?;
            refresh_new_messages(app, current_room.as_deref(), current_key, last_seen_id, user, current_input)?;
            return Ok(false);
        }
        KeyCode::Backspace => {
            current_input.pop();
        }
        KeyCode::Char(character) => {
            if !key_event.modifiers.contains(KeyModifiers::CONTROL) {
                current_input.push(character);
            }
        }
        _ => {}
    }

    redraw_prompt(user, current_room.as_deref(), current_input)?;
    Ok(false)
}

fn redraw_current_room(
    app: &MessengerApp<DemoCipher>,
    current_room: Option<&str>,
    key: &str,
    last_seen_id: &mut Option<u64>,
) -> Result<(), String> {
    clear_input_line()?;
    if let Some(room_name) = current_room {
        let messages = app.read_room_chat(room_name, key, None)?;
        print_messages(room_name, &messages);
        *last_seen_id = messages.last().map(|message| message.id);
    } else {
        *last_seen_id = None;
        println!("no active room");
    }
    Ok(())
}

fn refresh_new_messages(
    app: &MessengerApp<DemoCipher>,
    current_room: Option<&str>,
    key: &str,
    last_seen_id: &mut Option<u64>,
    user: &str,
    current_input: &str,
) -> Result<(), String> {
    if let Some(room_name) = current_room {
        let messages = app.read_room_chat(room_name, key, *last_seen_id)?;
        if !messages.is_empty() {
            clear_input_line()?;
            for message in &messages {
                println!("[{}] {}: {}", format_timestamp(message.timestamp), message.from, message.text);
            }
            *last_seen_id = messages.last().map(|message| message.id).or(*last_seen_id);
        }
    }

    redraw_prompt(user, current_room, current_input)
}

fn redraw_prompt(user: &str, room_name: Option<&str>, current_input: &str) -> Result<(), String> {
    let room = room_name.unwrap_or("lobby");
    let mut stdout = io::stdout();
    queue!(stdout, MoveToColumn(0), Clear(ClearType::CurrentLine))
        .map_err(|error| format!("failed to redraw prompt: {error}"))?;
    write!(stdout, "{user}@{room}> {current_input}")
        .map_err(|error| format!("failed to write prompt: {error}"))?;
    stdout.flush().map_err(|error| format!("failed to flush stdout: {error}"))
}

fn clear_input_line() -> Result<(), String> {
    execute!(io::stdout(), MoveToColumn(0), Clear(ClearType::CurrentLine))
        .map_err(|error| format!("failed to clear input line: {error}"))
}

fn print_messages(room_name: &str, messages: &[ChatMessageView]) {
    if messages.is_empty() {
        println!("no messages in room {room_name}");
        return;
    }
    println!("--- room: {room_name} ---");
    for message in messages {
        println!("[{}] {}: {}", format_timestamp(message.timestamp), message.from, message.text);
    }
    println!("-----------------");
}

fn print_rooms(rooms: Vec<RoomView>) {
    if rooms.is_empty() {
        println!("no rooms created yet");
        return;
    }
    println!("--- rooms ---");
    for room in rooms {
        println!("{} | owner: {} | members: {}/{}", room.name, room.owner, room.members, room.limit);
    }
    println!("-------------");
}

fn print_members(room_name: &str, members: Vec<String>) {
    println!("--- members in {room_name} ---");
    if members.is_empty() {
        println!("no members");
    } else {
        for member in members {
            println!("{member}");
        }
    }
    println!("----------------------");
}

fn parse_room_creation_args(input: &str) -> Result<(String, usize), String> {
    let mut parts = input.split_whitespace();
    let room_name = parts
        .next()
        .ok_or_else(|| "room name must not be empty".to_string())?
        .to_string();
    let limit = match parts.next() {
        Some(limit) => limit.parse::<usize>().map_err(|_| "invalid room limit".to_string())?,
        None => app::DEFAULT_ROOM_LIMIT,
    };
    Ok((room_name, limit))
}

fn format_timestamp(timestamp: u64) -> String {
    match DateTime::from_timestamp(timestamp as i64, 0) {
        Some(utc_time) => utc_time.with_timezone(&Local).format("%Y-%m-%d %H:%M:%S").to_string(),
        None => timestamp.to_string(),
    }
}

struct RawModeGuard;

impl RawModeGuard {
    fn enable() -> Result<Self, String> {
        terminal::enable_raw_mode().map_err(|error| format!("failed to enable raw mode: {error}"))?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}
