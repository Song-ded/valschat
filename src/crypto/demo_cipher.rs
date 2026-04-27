use crate::crypto::{Cipher, EncryptedPacket};
use std::time::{SystemTime, UNIX_EPOCH};

const INTERNAL_SENTINEL: u8 = 0x1f;
const HEADER: &[u8] = b"MK2|";
const DEFAULT_GARBAGE_MIN: usize = 3;
const DEFAULT_GARBAGE_MAX: usize = 24;
const PRINTABLE_ASCII_START: u8 = 0x20;
const PRINTABLE_ASCII_END: u8 = 0x7e;

pub struct DemoCipher;

impl DemoCipher {
    pub fn new() -> Self {
        Self
    }

    fn key_bytes<'a>(&self, key: &'a str) -> Result<&'a [u8], String> {
        let bytes = key.as_bytes();
        if bytes.is_empty() {
            return Err("key must not be empty".to_string());
        }
        if bytes.contains(&b'\n') || bytes.contains(&b'\r') {
            return Err("key must not contain newlines".to_string());
        }
        if bytes.contains(&INTERNAL_SENTINEL) {
            return Err("key contains an internal reserved byte".to_string());
        }
        Ok(bytes)
    }

    fn seed_from_plaintext(&self, key: &[u8], plaintext: &[u8]) -> Result<u64, String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "system clock is before unix epoch".to_string())?;

        let mut seed = now.as_nanos() as u64 ^ (plaintext.len() as u64).wrapping_mul(0x9e3779b97f4a7c15);
        for byte in key {
            seed ^= (*byte as u64).wrapping_mul(0x100000001b3);
            seed = seed.rotate_left(7);
        }

        if seed == 0 {
            seed = 0x1234_5678_9abc_def0;
        }
        Ok(seed)
    }

    fn next_u64(&self, state: &mut u64) -> u64 {
        let mut x = *state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *state = x;
        x
    }

    fn random_digit(&self, state: &mut u64) -> usize {
        ((self.next_u64(state) % 9) + 1) as usize
    }

    fn random_garbage_len(&self, state: &mut u64) -> usize {
        let span = DEFAULT_GARBAGE_MAX - DEFAULT_GARBAGE_MIN + 1;
        DEFAULT_GARBAGE_MIN + (self.next_u64(state) as usize % span)
    }

    fn filler_pool(&self, key: &[u8]) -> Vec<u8> {
        let mut pool = Vec::new();
        for byte in PRINTABLE_ASCII_START..=PRINTABLE_ASCII_END {
            if byte != INTERNAL_SENTINEL && !key.contains(&byte) {
                pool.push(byte);
            }
        }
        if pool.is_empty() {
            for byte in 1u8..=255 {
                if byte != INTERNAL_SENTINEL && !key.contains(&byte) {
                    pool.push(byte);
                }
            }
        }
        pool
    }

    fn random_filler(&self, state: &mut u64, pool: &[u8]) -> u8 {
        let index = (self.next_u64(state) as usize) % pool.len();
        pool[index]
    }

    fn add_random_words_auto(&self, output: &mut Vec<u8>, state: &mut u64, pool: &[u8]) {
        let count = self.random_garbage_len(state);
        for _ in 0..count {
            output.push(self.random_filler(state, pool));
        }
    }

    fn add_random_words_fixed(&self, output: &mut Vec<u8>, state: &mut u64, pool: &[u8], count: usize) {
        for _ in 0..count.saturating_sub(1) {
            output.push(self.random_filler(state, pool));
        }
    }

    fn to_hex_bytes(&self, plaintext: &[u8]) -> Vec<u8> {
        let mut hex = Vec::with_capacity(plaintext.len() * 2);
        for byte in plaintext {
            hex.push(b"0123456789abcdef"[(byte >> 4) as usize]);
            hex.push(b"0123456789abcdef"[(byte & 0x0f) as usize]);
        }
        hex
    }

    fn from_hex_bytes(&self, hex: &[u8]) -> Result<Vec<u8>, String> {
        if !hex.len().is_multiple_of(2) {
            return Err("decoded hex has odd length".to_string());
        }

        let mut output = Vec::with_capacity(hex.len() / 2);
        for index in (0..hex.len()).step_by(2) {
            let high = self.decode_hex_nibble(hex[index])?;
            let low = self.decode_hex_nibble(hex[index + 1])?;
            output.push((high << 4) | low);
        }
        Ok(output)
    }

    fn decode_hex_nibble(&self, byte: u8) -> Result<u8, String> {
        match byte {
            b'0'..=b'9' => Ok(byte - b'0'),
            b'a'..=b'f' => Ok(byte - b'a' + 10),
            b'A'..=b'F' => Ok(byte - b'A' + 10),
            _ => Err(format!("invalid hex nibble: {byte}")),
        }
    }

    fn encrypt_body(&self, key: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, String> {
        let mut state = self.seed_from_plaintext(key, plaintext)?;
        let pool = self.filler_pool(key);
        let payload = self.to_hex_bytes(plaintext);
        let mut output = Vec::new();

        for (index, payload_byte) in payload.iter().enumerate() {
            let marker = key[index % key.len()];
            let jump = self.random_digit(&mut state);
            self.add_random_words_auto(&mut output, &mut state, &pool);
            output.push(INTERNAL_SENTINEL);
            output.push(marker);
            output.push(b'0' + jump as u8);
            self.add_random_words_fixed(&mut output, &mut state, &pool, jump);
            output.push(*payload_byte);
            self.add_random_words_auto(&mut output, &mut state, &pool);
        }

        output.reverse();
        Ok(output)
    }

    fn decrypt_body(&self, key: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, String> {
        let mut reversed = ciphertext.to_vec();
        reversed.reverse();
        let mut payload = Vec::new();

        for index in 0..reversed.len() {
            if reversed[index] != INTERNAL_SENTINEL || index + 2 >= reversed.len() {
                continue;
            }

            let marker = reversed[index + 1];
            let digit = reversed[index + 2];
            let expected_marker = key[payload.len() % key.len()];
            if marker != expected_marker || !(b'1'..=b'9').contains(&digit) {
                continue;
            }

            let jump = (digit - b'0') as usize;
            let target_index = index + jump + 2;
            if target_index < reversed.len() {
                payload.push(reversed[target_index]);
            }
        }

        if payload.is_empty() {
            return Ok(self.wrong_key_bytes(&reversed));
        }

        match self.from_hex_bytes(&payload) {
            Ok(bytes) => Ok(bytes),
            Err(_) => Ok(self.wrong_key_bytes(&payload)),
        }
    }

    fn wrong_key_bytes(&self, garbage: &[u8]) -> Vec<u8> {
        let mut output = b"[wrong key] ".to_vec();
        output.extend_from_slice(String::from_utf8_lossy(garbage).as_bytes());
        output
    }
}

impl Cipher for DemoCipher {
    fn encrypt(&self, key: &str, plaintext: &[u8]) -> Result<EncryptedPacket, String> {
        let key = self.key_bytes(key)?;
        let body = self.encrypt_body(key, plaintext)?;
        let mut ciphertext = Vec::with_capacity(HEADER.len() + body.len());
        ciphertext.extend_from_slice(HEADER);
        ciphertext.extend_from_slice(&body);

        Ok(EncryptedPacket {
            nonce: Vec::new(),
            ciphertext,
        })
    }

    fn decrypt(&self, key: &str, packet: &EncryptedPacket) -> Result<Vec<u8>, String> {
        let key = self.key_bytes(key)?;
        let body = if packet.ciphertext.starts_with(HEADER) {
            &packet.ciphertext[HEADER.len()..]
        } else {
            &packet.ciphertext
        };
        self.decrypt_body(key, body)
    }
}

#[cfg(test)]
mod tests {
    use super::DemoCipher;
    use crate::crypto::Cipher;

    #[test]
    fn roundtrip_works_with_multichar_key() {
        let cipher = DemoCipher::new();
        let message = b"hello secure world";
        let packet = cipher.encrypt("start", message).unwrap();
        let decrypted = cipher.decrypt("start", &packet).unwrap();
        assert_eq!(decrypted, message);
    }

    #[test]
    fn spaces_digits_and_exclamation_roundtrip() {
        let cipher = DemoCipher::new();
        let message = b"hello ! room 42";
        let packet = cipher.encrypt("!42@room", message).unwrap();
        let decrypted = cipher.decrypt("!42@room", &packet).unwrap();
        assert_eq!(decrypted, message);
    }

    #[test]
    fn full_key_changes_result() {
        let cipher = DemoCipher::new();
        let message = b"hello secure world";
        let packet = cipher.encrypt("start", message).unwrap();
        let decrypted = cipher.decrypt("start", &packet).unwrap();
        let wrong = cipher.decrypt("stark", &packet).unwrap();
        assert_eq!(decrypted, message);
        assert_ne!(wrong, message);
    }

    #[test]
    fn same_first_symbol_but_different_rest_fails() {
        let cipher = DemoCipher::new();
        let message = b"abcdef123456";
        let packet = cipher.encrypt("s-room-a", message).unwrap();
        let wrong = cipher.decrypt("s-room-b", &packet).unwrap();
        assert_ne!(wrong, message);
    }
}
