#[derive(Clone, Debug)]
pub struct MessageRecord {
    pub timestamp: u64,
    pub room: String,
    pub from: String,
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct DecryptedMessage {
    pub timestamp: u64,
    pub from: String,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct RoomRecord {
    pub name: String,
    pub owner: String,
    pub limit: usize,
}

#[derive(Clone, Debug)]
pub struct RoomMembership {
    pub room: String,
    pub user: String,
}

#[derive(Clone, Debug)]
pub struct RoomBan {
    pub room: String,
    pub user: String,
}
