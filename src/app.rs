use crate::crypto::{Cipher, EncryptedPacket};
use crate::model::{DecryptedMessage, MessageRecord, RoomBan, RoomMembership, RoomRecord};
use crate::store::ChatStore;
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_CHAT_KEY: &str = "start";
pub const DEFAULT_ROOM_LIMIT: usize = 25;

pub struct MessengerApp<S, C> {
    store: S,
    cipher: C,
}

pub struct RoomView {
    pub name: String,
    pub owner: String,
    pub limit: usize,
    pub members: usize,
}

impl<S, C> MessengerApp<S, C>
where
    S: ChatStore,
    C: Cipher,
{
    pub fn new(store: S, cipher: C) -> Self {
        Self { store, cipher }
    }

    pub fn create_room(
        &self,
        owner: &str,
        room_name: &str,
        limit: usize,
    ) -> Result<(), String> {
        validate_user(owner)?;
        validate_room_name(room_name)?;
        if limit == 0 {
            return Err("room limit must be greater than zero".to_string());
        }

        let mut rooms = self.store.read_rooms()?;
        if rooms.iter().any(|room| room.name == room_name) {
            return Err("room already exists".to_string());
        }

        rooms.push(RoomRecord {
            name: room_name.to_string(),
            owner: owner.to_string(),
            limit,
        });
        self.store.save_rooms(&rooms)?;

        let mut memberships = self.store.read_memberships()?;
        memberships.push(RoomMembership {
            room: room_name.to_string(),
            user: owner.to_string(),
        });
        self.store.save_memberships(&memberships)
    }

    pub fn join_room(&self, user: &str, room_name: &str) -> Result<(), String> {
        validate_user(user)?;
        let rooms = self.store.read_rooms()?;
        let room = rooms
            .iter()
            .find(|room| room.name == room_name)
            .ok_or_else(|| "room not found".to_string())?;

        let bans = self.store.read_bans()?;
        if bans.iter().any(|ban| ban.room == room_name && ban.user == user) {
            return Err("you are banned from this room".to_string());
        }

        let mut memberships = self.store.read_memberships()?;
        if memberships
            .iter()
            .any(|membership| membership.room == room_name && membership.user == user)
        {
            return Ok(());
        }

        let member_count = memberships
            .iter()
            .filter(|membership| membership.room == room_name)
            .count();
        if member_count >= room.limit {
            return Err("room is full".to_string());
        }

        memberships.push(RoomMembership {
            room: room_name.to_string(),
            user: user.to_string(),
        });
        self.store.save_memberships(&memberships)
    }

    pub fn leave_room(&self, user: &str, room_name: &str) -> Result<(), String> {
        validate_user(user)?;
        let rooms = self.store.read_rooms()?;
        let room = rooms
            .iter()
            .find(|room| room.name == room_name)
            .ok_or_else(|| "room not found".to_string())?;

        let mut memberships = self.store.read_memberships()?;
        let before = memberships.len();
        memberships.retain(|membership| !(membership.room == room_name && membership.user == user));
        if memberships.len() == before {
            return Err("you are not in this room".to_string());
        }

        if room.owner == user
            && memberships.iter().any(|membership| membership.room == room_name)
        {
            return Err("owner cannot leave while other users are still in the room".to_string());
        }

        self.store.save_memberships(&memberships)?;
        self.cleanup_empty_room(room_name)
    }

    pub fn set_room_limit(&self, owner: &str, room_name: &str, limit: usize) -> Result<(), String> {
        validate_user(owner)?;
        if limit == 0 {
            return Err("room limit must be greater than zero".to_string());
        }

        let memberships = self.store.read_memberships()?;
        let current_members = memberships
            .iter()
            .filter(|membership| membership.room == room_name)
            .count();
        if limit < current_members {
            return Err("new limit is smaller than current room size".to_string());
        }

        let mut rooms = self.store.read_rooms()?;
        let room = rooms
            .iter_mut()
            .find(|room| room.name == room_name)
            .ok_or_else(|| "room not found".to_string())?;
        if room.owner != owner {
            return Err("only the room owner can change the limit".to_string());
        }
        room.limit = limit;
        self.store.save_rooms(&rooms)
    }

    pub fn kick_user(&self, owner: &str, room_name: &str, target: &str) -> Result<(), String> {
        validate_user(owner)?;
        validate_user(target)?;
        let rooms = self.store.read_rooms()?;
        let room = rooms
            .iter()
            .find(|room| room.name == room_name)
            .ok_or_else(|| "room not found".to_string())?;
        if room.owner != owner {
            return Err("only the room owner can kick users".to_string());
        }
        if room.owner == target {
            return Err("owner cannot kick themselves".to_string());
        }

        let mut memberships = self.store.read_memberships()?;
        let before = memberships.len();
        memberships.retain(|membership| !(membership.room == room_name && membership.user == target));
        if memberships.len() == before {
            return Err("user is not in this room".to_string());
        }
        self.store.save_memberships(&memberships)?;
        self.cleanup_empty_room(room_name)
    }

    pub fn ban_user(&self, owner: &str, room_name: &str, target: &str) -> Result<(), String> {
        validate_user(owner)?;
        validate_user(target)?;
        let rooms = self.store.read_rooms()?;
        let room = rooms
            .iter()
            .find(|room| room.name == room_name)
            .ok_or_else(|| "room not found".to_string())?;
        if room.owner != owner {
            return Err("only the room owner can ban users".to_string());
        }
        if room.owner == target {
            return Err("owner cannot ban themselves".to_string());
        }

        let mut bans = self.store.read_bans()?;
        if !bans.iter().any(|ban| ban.room == room_name && ban.user == target) {
            bans.push(RoomBan {
                room: room_name.to_string(),
                user: target.to_string(),
            });
            self.store.save_bans(&bans)?;
        }

        let mut memberships = self.store.read_memberships()?;
        memberships.retain(|membership| !(membership.room == room_name && membership.user == target));
        self.store.save_memberships(&memberships)?;
        self.cleanup_empty_room(room_name)
    }

    pub fn list_rooms(&self) -> Result<Vec<RoomView>, String> {
        let rooms = self.store.read_rooms()?;
        let memberships = self.store.read_memberships()?;
        let mut output = Vec::with_capacity(rooms.len());

        for room in rooms {
            let members = memberships
                .iter()
                .filter(|membership| membership.room == room.name)
                .count();
            output.push(RoomView {
                name: room.name,
                owner: room.owner,
                limit: room.limit,
                members,
            });
        }

        output.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(output)
    }

    pub fn list_members(&self, room_name: &str) -> Result<Vec<String>, String> {
        ensure_room_exists(&self.store, room_name)?;
        let mut members: Vec<String> = self
            .store
            .read_memberships()?
            .into_iter()
            .filter(|membership| membership.room == room_name)
            .map(|membership| membership.user)
            .collect();
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
        ensure_member(&self.store, room_name, from)?;
        if key.is_empty() {
            return Err("key must not be empty".to_string());
        }
        if message.is_empty() {
            return Err("message must not be empty".to_string());
        }

        let packet = self.cipher.encrypt(key, message.as_bytes())?;
        let record = MessageRecord {
            timestamp: unix_time_now()?,
            room: room_name.to_string(),
            from: from.to_string(),
            nonce: packet.nonce,
            ciphertext: packet.ciphertext,
        };

        self.store.append_message(&record)
    }

    pub fn read_room_chat(&self, room_name: &str, key: &str) -> Result<Vec<DecryptedMessage>, String> {
        validate_room_name(room_name)?;
        if key.is_empty() {
            return Err("key must not be empty".to_string());
        }

        let mut records = self.store.read_messages()?;
        records.retain(|record| record.room == room_name);
        records.sort_by_key(|record| record.timestamp);

        let mut output = Vec::with_capacity(records.len());
        for record in records {
            let packet = EncryptedPacket {
                nonce: record.nonce.clone(),
                ciphertext: record.ciphertext.clone(),
            };
            let plaintext = self.cipher.decrypt(key, &packet)?;
            let text = String::from_utf8_lossy(&plaintext).to_string();
            output.push(DecryptedMessage {
                timestamp: record.timestamp,
                from: record.from,
                text,
            });
        }

        Ok(output)
    }

    fn cleanup_empty_room(&self, room_name: &str) -> Result<(), String> {
        let memberships = self.store.read_memberships()?;
        if memberships.iter().any(|membership| membership.room == room_name) {
            return Ok(());
        }

        let mut rooms = self.store.read_rooms()?;
        rooms.retain(|room| room.name != room_name);
        self.store.save_rooms(&rooms)?;

        let mut bans = self.store.read_bans()?;
        bans.retain(|ban| ban.room != room_name);
        self.store.save_bans(&bans)?;

        let mut messages = self.store.read_messages()?;
        messages.retain(|message| message.room != room_name);
        self.store.save_messages(&messages)
    }
}

fn ensure_room_exists<S: ChatStore>(store: &S, room_name: &str) -> Result<(), String> {
    if store.read_rooms()?.iter().any(|room| room.name == room_name) {
        Ok(())
    } else {
        Err("room not found".to_string())
    }
}

fn ensure_member<S: ChatStore>(store: &S, room_name: &str, user: &str) -> Result<(), String> {
    ensure_room_exists(store, room_name)?;
    if store
        .read_memberships()?
        .iter()
        .any(|membership| membership.room == room_name && membership.user == user)
    {
        Ok(())
    } else {
        Err("you are not a member of this room".to_string())
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

fn unix_time_now() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| "system clock is before unix epoch".to_string())
}
