use crate::crypto::{Cipher, EncryptedPacket};
use std::time::{SystemTime, UNIX_EPOCH};

const HEADER: &[u8] = b"MK2|";
const TAG_LEN: usize = 8;
const DEFAULT_SIZE: usize = 4;
const PAYLOAD_BASE: u8 = 0x10;
const MIXED_POOL: &[u8] = b"123456791234567912345679123456791234567912345679123456791234567912345679123456791234567912345679123456791234567912345679123456791234567912345679qwertyuuiop[]asdfghjkl;'zxcvbnm,./!@#%$^&*$(^)&(+_|~!!!!!!!.QWERTYUIOPASDFGHJKLZXCVBNM";

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
        Ok(bytes)
    }

    fn seed_from_plaintext(&self, key: &[u8], plaintext: &[u8]) -> Result<u64, String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "system clock is before unix epoch".to_string())?;

        let mut seed = now.as_nanos() as u64 ^ (plaintext.len() as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        for byte in key {
            seed ^= (*byte as u64).wrapping_mul(0x1000_0000_01b3);
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

    fn random_range(&self, state: &mut u64, start: usize, end: usize) -> usize {
        if end <= start {
            return start;
        }
        start + (self.next_u64(state) as usize % (end - start + 1))
    }

    fn random_jump(&self, state: &mut u64) -> usize {
        loop {
            let value = self.random_range(state, 1, 9);
            if value != 8 {
                return value;
            }
        }
    }

    fn filtered_pool(&self, key: &[u8]) -> Vec<u8> {
        let mut pool = MIXED_POOL
            .iter()
            .copied()
            .filter(|byte| !key.contains(byte))
            .filter(|byte| !(*byte >= PAYLOAD_BASE && *byte < PAYLOAD_BASE + 16))
            .collect::<Vec<_>>();

        if pool.is_empty() {
            pool.extend(
                (1u8..=255)
                    .filter(|byte| !key.contains(byte))
                    .filter(|byte| !(*byte >= PAYLOAD_BASE && *byte < PAYLOAD_BASE + 16)),
            );
        }

        pool
    }

    fn random_from_pool(&self, state: &mut u64, pool: &[u8]) -> u8 {
        let index = self.next_u64(state) as usize % pool.len();
        pool[index]
    }

    fn add_random_words_auto(&self, output: &mut Vec<u8>, state: &mut u64, mixed_pool: &[u8]) {
        let count = self.random_range(state, DEFAULT_SIZE / 2, DEFAULT_SIZE * 10);
        for _ in 0..count {
            output.push(self.random_from_pool(state, mixed_pool));
        }
    }

    fn add_random_words_fixed(
        &self,
        output: &mut Vec<u8>,
        state: &mut u64,
        count: usize,
        mixed_pool: &[u8],
    ) {
        for _ in 0..count.saturating_sub(1) {
            output.push(self.random_from_pool(state, mixed_pool));
        }
    }

    fn to_payload_bytes(&self, plaintext: &[u8]) -> Vec<u8> {
        let mut encoded = Vec::with_capacity(plaintext.len() * 2);
        for byte in plaintext {
            encoded.push(PAYLOAD_BASE + (byte >> 4));
            encoded.push(PAYLOAD_BASE + (byte & 0x0f));
        }
        encoded
    }

    fn from_payload_bytes(&self, payload: &[u8]) -> Result<Vec<u8>, String> {
        if !payload.len().is_multiple_of(2) {
            return Err("decoded payload has odd length".to_string());
        }

        let mut output = Vec::with_capacity(payload.len() / 2);
        for index in (0..payload.len()).step_by(2) {
            let high = self.decode_payload_nibble(payload[index])?;
            let low = self.decode_payload_nibble(payload[index + 1])?;
            output.push((high << 4) | low);
        }
        Ok(output)
    }

    fn decode_payload_nibble(&self, byte: u8) -> Result<u8, String> {
        if (PAYLOAD_BASE..PAYLOAD_BASE + 16).contains(&byte) {
            Ok(byte - PAYLOAD_BASE)
        } else {
            Err(format!("invalid payload nibble: {byte}"))
        }
    }

    fn compute_tag(&self, key: &[u8], body: &[u8]) -> [u8; TAG_LEN] {
        let mut left = 0x243f_6a88_85a3_08d3u64;
        let mut right = 0x1319_8a2e_0370_7344u64;

        for byte in key {
            left ^= *byte as u64;
            left = left.rotate_left(5).wrapping_mul(0x1000_0000_01b3);
            right ^= (*byte as u64) << 1;
            right = right.rotate_left(9).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        }

        for byte in body {
            left ^= *byte as u64;
            left = left.rotate_left(7).wrapping_add(0xa5a5_a5a5_a5a5_a5a5);
            right ^= (*byte as u64) << 1;
            right = right.rotate_left(11).wrapping_add(0x3c6e_f372_fe94_f82b);
        }

        let mixed = left ^ right.rotate_left(17) ^ (body.len() as u64).wrapping_mul(0x27d4_eb2f_1656_67c5);
        mixed.to_le_bytes()
    }

    fn encrypt_body(&self, key: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, String> {
        let mut state = self.seed_from_plaintext(key, plaintext)?;
        let mixed_pool = self.filtered_pool(key);
        let payload = self.to_payload_bytes(plaintext);
        let mut output = Vec::new();

        for (index, payload_byte) in payload.iter().enumerate() {
            let marker = key[index % key.len()];
            let jump = self.random_jump(&mut state);
            self.add_random_words_auto(&mut output, &mut state, &mixed_pool);
            output.push(marker);
            output.push(b'0' + jump as u8);
            self.add_random_words_fixed(&mut output, &mut state, jump, &mixed_pool);
            output.push(*payload_byte);
            self.add_random_words_auto(&mut output, &mut state, &mixed_pool);
        }

        output.reverse();
        Ok(output)
    }

    fn decrypt_body(&self, key: &[u8], ciphertext: &[u8]) -> Vec<u8> {
        let mut reversed = ciphertext.to_vec();
        reversed.reverse();
        let mut payload = Vec::new();

        for index in 0..reversed.len() {
            if index + 1 >= reversed.len() {
                continue;
            }

            let expected_marker = key[payload.len() % key.len()];
            if reversed[index] != expected_marker {
                continue;
            }

            let digit = reversed[index + 1];
            if !(b'1'..=b'9').contains(&digit) {
                continue;
            }

            let jump = (digit - b'0') as usize;
            let target_index = index + jump + 1;
            if target_index < reversed.len() {
                let candidate = reversed[target_index];
                if (PAYLOAD_BASE..PAYLOAD_BASE + 16).contains(&candidate) {
                    payload.push(candidate);
                }
            }
        }

        if payload.is_empty() {
            return reversed;
        }

        self.from_payload_bytes(&payload).unwrap_or(payload)
    }
}

impl Cipher for DemoCipher {
    fn encrypt(&self, key: &str, plaintext: &[u8]) -> Result<EncryptedPacket, String> {
        let key = self.key_bytes(key)?;
        let body = self.encrypt_body(key, plaintext)?;
        let tag = self.compute_tag(key, &body);
        let mut ciphertext = Vec::with_capacity(HEADER.len() + TAG_LEN + body.len());
        ciphertext.extend_from_slice(HEADER);
        ciphertext.extend_from_slice(&tag);
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
        if body.len() < TAG_LEN {
            return Ok(body.to_vec());
        }
        let (tag, cipher_body) = body.split_at(TAG_LEN);
        let expected = self.compute_tag(key, cipher_body);
        if tag != expected {
            return Ok(cipher_body.to_vec());
        }
        Ok(self.decrypt_body(key, cipher_body))
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

    #[test]
    fn single_symbol_key_still_roundtrips() {
        let cipher = DemoCipher::new();
        let message = b"single marker key works too";
        let packet = cipher.encrypt("8", message).unwrap();
        let decrypted = cipher.decrypt("8", &packet).unwrap();
        assert_eq!(decrypted, message);
    }

    #[test]
    fn tampered_ciphertext_fails_integrity() {
        let cipher = DemoCipher::new();
        let message = b"integrity matters";
        let mut packet = cipher.encrypt("start", message).unwrap();
        let last = packet.ciphertext.len() - 1;
        packet.ciphertext[last] ^= 1;
        let decrypted = cipher.decrypt("start", &packet).unwrap();
        assert_ne!(decrypted, message);
    }
}
