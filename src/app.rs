use crate::crypto::{Cipher, EncryptedPacket};
use crate::store::{RoomSummary, ServerApi, StoredMessage};

pub const DEFAULT_CHAT_KEY: &str = "start";
pub const DEFAULT_ROOM_LIMIT: usize = 25;

pub struct MessengerApp<C> {
    server: ServerApi,
    cipher: C,
}

pub struct RoomView {
    pub name: String,
    pub owner: String,
    pub limit: usize,
    pub members: usize,
}

pub struct ChatMessageView {
    pub id: u64,
    pub timestamp: u64,
    pub from: String,
    pub text: String,
}

impl<C> MessengerApp<C>
where
    C: Cipher,
{
    pub fn new(server: ServerApi, cipher: C) -> Self {
        Self { server, cipher }
    }

    pub fn create_room(&self, owner: &str, room_name: &str, limit: usize) -> Result<(), String> {
        validate_user(owner)?;
        validate_room_name(room_name)?;
        if limit == 0 {
            return Err("room limit must be greater than zero".to_string());
        }
        self.server.create_room(owner, room_name, limit)
    }

    pub fn join_room(&self, user: &str, room_name: &str) -> Result<(), String> {
        validate_user(user)?;
        validate_room_name(room_name)?;
        self.server.join_room(user, room_name)
    }

    pub fn leave_room(&self, user: &str, room_name: &str) -> Result<(), String> {
        validate_user(user)?;
        validate_room_name(room_name)?;
        self.server.leave_room(user, room_name)
    }

    pub fn set_room_limit(&self, owner: &str, room_name: &str, limit: usize) -> Result<(), String> {
        validate_user(owner)?;
        validate_room_name(room_name)?;
        if limit == 0 {
            return Err("room limit must be greater than zero".to_string());
        }
        self.server.set_room_limit(owner, room_name, limit)
    }

    pub fn kick_user(&self, owner: &str, room_name: &str, target: &str) -> Result<(), String> {
        validate_user(owner)?;
        validate_user(target)?;
        validate_room_name(room_name)?;
        self.server.kick_user(owner, room_name, target)
    }

    pub fn ban_user(&self, owner: &str, room_name: &str, target: &str) -> Result<(), String> {
        validate_user(owner)?;
        validate_user(target)?;
        validate_room_name(room_name)?;
        self.server.ban_user(owner, room_name, target)
    }

    pub fn list_rooms(&self) -> Result<Vec<RoomView>, String> {
        let mut rooms = self
            .server
            .list_rooms()?
            .into_iter()
            .map(map_room)
            .collect::<Vec<_>>();
        rooms.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(rooms)
    }

    pub fn list_members(&self, room_name: &str) -> Result<Vec<String>, String> {
        validate_room_name(room_name)?;
        let mut members = self.server.list_members(room_name)?;
        members.sort();
        Ok(members)
    }

    pub fn send_message(
        &self,
        room_name: &str,
        from: &str,
        key: &str,
        message: &str,
    ) -> Result<(), String> {
        validate_user(from)?;
        validate_room_name(room_name)?;
        if key.is_empty() {
            return Err("key must not be empty".to_string());
        }
        if message.is_empty() {
            return Err("message must not be empty".to_string());
        }

        let packet = self.cipher.encrypt(key, message.as_bytes())?;
        self.server
            .send_message(room_name, from, &hex_encode(&packet.ciphertext))
    }

    pub fn read_room_chat(
        &self,
        room_name: &str,
        key: &str,
        after_id: Option<u64>,
    ) -> Result<Vec<ChatMessageView>, String> {
        validate_room_name(room_name)?;
        if key.is_empty() {
            return Err("key must not be empty".to_string());
        }

        let mut output = Vec::new();
        for message in self.server.read_messages(room_name, after_id)? {
            output.push(self.decrypt_message(key, message)?);
        }
        output.sort_by_key(|message| (message.timestamp, message.id));
        Ok(output)
    }

    fn decrypt_message(&self, key: &str, message: StoredMessage) -> Result<ChatMessageView, String> {
        let ciphertext = hex_decode(&message.ciphertext)?;
        let plaintext = self.cipher.decrypt(
            key,
            &EncryptedPacket {
                nonce: Vec::new(),
                ciphertext,
            },
        )?;

        Ok(ChatMessageView {
            id: message.id,
            timestamp: message.timestamp,
            from: message.from,
            text: String::from_utf8_lossy(&plaintext).to_string(),
        })
    }
}

fn map_room(room: RoomSummary) -> RoomView {
    RoomView {
        name: room.name,
        owner: room.owner,
        limit: room.limit,
        members: room.members,
    }
}

fn validate_user(user: &str) -> Result<(), String> {
    if user.is_empty() {
        return Err("username must not be empty".to_string());
    }
    if user.contains('\t') || user.contains('\n') || user.contains('\r') {
        return Err("username must not contain tabs or newlines".to_string());
    }
    Ok(())
}

fn validate_room_name(room_name: &str) -> Result<(), String> {
    if room_name.is_empty() {
        return Err("room name must not be empty".to_string());
    }
    if room_name.contains('\t') || room_name.contains('\n') || room_name.contains('\r') {
        return Err("room name must not contain tabs or newlines".to_string());
    }
    Ok(())
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
