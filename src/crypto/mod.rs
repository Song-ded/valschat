pub mod demo_cipher;

#[derive(Clone, Debug)]
pub struct EncryptedPacket {
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

pub trait Cipher {
    fn encrypt(&self, key: &str, plaintext: &[u8]) -> Result<EncryptedPacket, String>;
    fn decrypt(&self, key: &str, packet: &EncryptedPacket) -> Result<Vec<u8>, String>;
}
