use crate::model::{MessageRecord, RoomBan, RoomMembership, RoomRecord};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

pub trait ChatStore {
    fn append_message(&self, record: &MessageRecord) -> Result<(), String>;
    fn read_messages(&self) -> Result<Vec<MessageRecord>, String>;
    fn save_messages(&self, messages: &[MessageRecord]) -> Result<(), String>;
    fn save_rooms(&self, rooms: &[RoomRecord]) -> Result<(), String>;
    fn read_rooms(&self) -> Result<Vec<RoomRecord>, String>;
    fn save_memberships(&self, memberships: &[RoomMembership]) -> Result<(), String>;
    fn read_memberships(&self) -> Result<Vec<RoomMembership>, String>;
    fn save_bans(&self, bans: &[RoomBan]) -> Result<(), String>;
    fn read_bans(&self) -> Result<Vec<RoomBan>, String>;
}

pub struct FileChatStore {
    base_dir: PathBuf,
}

impl FileChatStore {
    pub fn new<P: Into<PathBuf>>(base_dir: P) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }

    fn ensure_dir(&self) -> Result<(), String> {
        fs::create_dir_all(&self.base_dir).map_err(|error| {
            format!(
                "failed to create storage directory {}: {error}",
                self.base_dir.display()
            )
        })
    }

    fn messages_path(&self) -> PathBuf {
        self.base_dir.join("messages.log")
    }

    fn rooms_path(&self) -> PathBuf {
        self.base_dir.join("rooms.log")
    }

    fn memberships_path(&self) -> PathBuf {
        self.base_dir.join("memberships.log")
    }

    fn bans_path(&self) -> PathBuf {
        self.base_dir.join("bans.log")
    }

    fn rewrite_lines(&self, path: &Path, lines: &[String]) -> Result<(), String> {
        self.ensure_dir()?;
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)
            .map_err(|error| format!("failed to open {}: {error}", path.display()))?;

        for line in lines {
            file.write_all(line.as_bytes())
                .map_err(|error| format!("failed to write {}: {error}", path.display()))?;
            file.write_all(b"\n")
                .map_err(|error| format!("failed to write {}: {error}", path.display()))?;
        }

        Ok(())
    }

    fn read_lines(&self, path: &Path) -> Result<Vec<String>, String> {
        if !path.exists() {
            return Ok(Vec::new());
        }

        let file = OpenOptions::new()
            .read(true)
            .open(path)
            .map_err(|error| format!("failed to open {}: {error}", path.display()))?;
        let reader = BufReader::new(file);
        let mut lines = Vec::new();

        for (line_index, line_result) in reader.lines().enumerate() {
            let line = line_result.map_err(|error| {
                format!(
                    "failed to read line {} in {}: {error}",
                    line_index + 1,
                    path.display()
                )
            })?;
            if !line.trim().is_empty() {
                lines.push(line);
            }
        }

        Ok(lines)
    }
}

impl ChatStore for FileChatStore {
    fn append_message(&self, record: &MessageRecord) -> Result<(), String> {
        self.ensure_dir()?;
        let path = self.messages_path();
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|error| format!("failed to open {}: {error}", path.display()))?;

        let line = format!(
            "{}\t{}\t{}\t{}\t{}",
            record.timestamp,
            record.room,
            record.from,
            hex_encode(&record.nonce),
            hex_encode(&record.ciphertext)
        );

        file.write_all(line.as_bytes())
            .map_err(|error| format!("failed to write {}: {error}", path.display()))?;
        file.write_all(b"\n")
            .map_err(|error| format!("failed to write {}: {error}", path.display()))
    }

    fn read_messages(&self) -> Result<Vec<MessageRecord>, String> {
        let path = self.messages_path();
        let lines = self.read_lines(&path)?;
        let mut messages = Vec::new();

        for (line_index, line) in lines.iter().enumerate() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() != 5 {
                continue;
            }

            let timestamp = parts[0].parse::<u64>().map_err(|_| {
                format!(
                    "invalid timestamp on line {} in {}",
                    line_index + 1,
                    path.display()
                )
            })?;

            messages.push(MessageRecord {
                timestamp,
                room: parts[1].to_string(),
                from: parts[2].to_string(),
                nonce: hex_decode(parts[3]).map_err(|error| {
                    format!(
                        "invalid nonce on line {} in {}: {error}",
                        line_index + 1,
                        path.display()
                    )
                })?,
                ciphertext: hex_decode(parts[4]).map_err(|error| {
                    format!(
                        "invalid ciphertext on line {} in {}: {error}",
                        line_index + 1,
                        path.display()
                    )
                })?,
            });
        }

        Ok(messages)
    }

    fn save_messages(&self, messages: &[MessageRecord]) -> Result<(), String> {
        let lines: Vec<String> = messages
            .iter()
            .map(|record| {
                format!(
                    "{}\t{}\t{}\t{}\t{}",
                    record.timestamp,
                    record.room,
                    record.from,
                    hex_encode(&record.nonce),
                    hex_encode(&record.ciphertext)
                )
            })
            .collect();
        self.rewrite_lines(&self.messages_path(), &lines)
    }

    fn save_rooms(&self, rooms: &[RoomRecord]) -> Result<(), String> {
        let lines: Vec<String> = rooms
            .iter()
            .map(|room| format!("{}\t{}\t{}", room.name, room.owner, room.limit))
            .collect();
        self.rewrite_lines(&self.rooms_path(), &lines)
    }

    fn read_rooms(&self) -> Result<Vec<RoomRecord>, String> {
        let path = self.rooms_path();
        let lines = self.read_lines(&path)?;
        let mut rooms = Vec::new();

        for (line_index, line) in lines.iter().enumerate() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() != 3 {
                continue;
            }
            rooms.push(RoomRecord {
                name: parts[0].to_string(),
                owner: parts[1].to_string(),
                limit: parts[2].parse::<usize>().map_err(|_| {
                    format!(
                        "invalid room limit on line {} in {}",
                        line_index + 1,
                        path.display()
                    )
                })?,
            });
        }

        Ok(rooms)
    }

    fn save_memberships(&self, memberships: &[RoomMembership]) -> Result<(), String> {
        let lines: Vec<String> = memberships
            .iter()
            .map(|membership| format!("{}\t{}", membership.room, membership.user))
            .collect();
        self.rewrite_lines(&self.memberships_path(), &lines)
    }

    fn read_memberships(&self) -> Result<Vec<RoomMembership>, String> {
        let path = self.memberships_path();
        let lines = self.read_lines(&path)?;
        let mut memberships = Vec::new();

        for line in lines {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() != 2 {
                continue;
            }
            memberships.push(RoomMembership {
                room: parts[0].to_string(),
                user: parts[1].to_string(),
            });
        }

        Ok(memberships)
    }

    fn save_bans(&self, bans: &[RoomBan]) -> Result<(), String> {
        let lines: Vec<String> = bans
            .iter()
            .map(|ban| format!("{}\t{}", ban.room, ban.user))
            .collect();
        self.rewrite_lines(&self.bans_path(), &lines)
    }

    fn read_bans(&self) -> Result<Vec<RoomBan>, String> {
        let path = self.bans_path();
        let lines = self.read_lines(&path)?;
        let mut bans = Vec::new();

        for line in lines {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() != 2 {
                continue;
            }
            bans.push(RoomBan {
                room: parts[0].to_string(),
                user: parts[1].to_string(),
            });
        }

        Ok(bans)
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(b"0123456789abcdef"[(byte >> 4) as usize]));
        output.push(char::from(b"0123456789abcdef"[(byte & 0x0f) as usize]));
    }
    output
}

fn hex_decode(input: &str) -> Result<Vec<u8>, String> {
    if !input.len().is_multiple_of(2) {
        return Err("hex string must have even length".to_string());
    }

    let mut output = Vec::with_capacity(input.len() / 2);
    let bytes = input.as_bytes();

    for index in (0..bytes.len()).step_by(2) {
        let high = decode_nibble(bytes[index])?;
        let low = decode_nibble(bytes[index + 1])?;
        output.push((high << 4) | low);
    }

    Ok(output)
}

fn decode_nibble(byte: u8) -> Result<u8, String> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(format!("invalid hex byte: {}", byte as char)),
    }
}

#[cfg(test)]
mod tests {
    use super::{hex_decode, hex_encode};

    #[test]
    fn hex_roundtrip_works() {
        let source = b"\x00\x7f\x80\xff";
        let encoded = hex_encode(source);
        let decoded = hex_decode(&encoded).unwrap();
        assert_eq!(decoded, source);
    }
}
