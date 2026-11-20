//! Each player's skin, for drawing them in a pose. The client's API doesn't name a player's skin
//! texture, but the server has each player's profile: its `textures` property (base64 JSON) holds
//! the skin's URL and model, and the game names a downloaded skin `skins/` and the SHA-1 of the
//! URL's last path part (as UTF-16 code units, `SkinManager.TextureCache`). The server works the
//! name out and tells the clients; a skin not downloaded yet just draws the default for a while.
//! Pure.

/// Where the launcher's skin of an offline name is kept, for this window's own player.
pub const OFFLINE_SKIN: &str = "horizon/offline_skin";

/// A player's skin as the server tells it: their name, the texture, and whether it is slim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkinMsg {
    pub name: String,
    pub texture: String,
    pub slim: bool,
}

impl SkinMsg {
    /// One byte for slim, the name, a 0, the texture.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![u8::from(self.slim)];
        out.extend_from_slice(self.name.as_bytes());
        out.push(0);
        out.extend_from_slice(self.texture.as_bytes());
        out
    }

    pub fn decode(data: &[u8]) -> Option<SkinMsg> {
        let (&slim, rest) = data.split_first()?;
        let at = rest.iter().position(|&b| b == 0)?;
        let name = std::str::from_utf8(&rest[..at]).ok()?.to_owned();
        let texture = std::str::from_utf8(&rest[at + 1..]).ok()?.to_owned();
        if slim > 1 || name.is_empty() || texture.is_empty() {
            return None;
        }
        Some(SkinMsg { name, texture, slim: slim == 1 })
    }
}

/// The skin texture and model a profile's `textures` property names; None without a skin (the
/// player shows a default one).
pub fn skin_of_textures(property: &str) -> Option<(String, bool)> {
    let json = String::from_utf8(base64(property)?).ok()?;
    let skin = object_after(&json, "\"SKIN\"")?;
    let url = string_after(skin, "\"url\"")?;
    let slim = string_after(skin, "\"model\"").is_some_and(|m| m == "slim");
    Some((location(&texture_hash(&url)?), slim))
}

/// The last part of the URL's path, without its extension.
fn texture_hash(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let path = &rest[rest.find('/').unwrap_or(rest.len())..];
    let path = &path[..path.find(['?', '#']).unwrap_or(path.len())];
    let name = &path[path.rfind(['/', '\\']).map_or(0, |i| i + 1)..];
    Some(name[..name.rfind('.').unwrap_or(name.len())].to_owned())
}

/// `skins/` and the SHA-1 of `hash`'s UTF-16 code units, little-endian.
fn location(hash: &str) -> String {
    let units: Vec<u8> = hash.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut out = String::from("skins/");
    for b in sha1(&units) {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// The `{...}` that follows `key` in `json`, braces matched (strings skipped).
fn object_after<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let from = json.find(key)? + key.len();
    let open = from + json[from..].find('{')?;
    let mut depth = 0;
    let mut in_string = false;
    let mut escaped = false;
    for (i, c) in json[open..].char_indices() {
        if in_string {
            match c {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&json[open..=open + i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// The string value of `key` in `json`, simple escapes undone.
fn string_after(json: &str, key: &str) -> Option<String> {
    let from = json.find(key)? + key.len();
    let rest = json[from..].trim_start().strip_prefix(':')?.trim_start().strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => out.push(chars.next()?),
            c => out.push(c),
        }
    }
    None
}

/// Standard base64 (padding and whitespace allowed).
fn base64(text: &str) -> Option<Vec<u8>> {
    let value = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' | b'-' => Some(62),
        b'/' | b'_' => Some(63),
        _ => None,
    };
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0);
    for c in text.bytes().filter(|c| !c.is_ascii_whitespace() && *c != b'=') {
        acc = (acc << 6) | u32::from(value(c)?);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
    let mut message = data.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for chunk in message.chunks(64) {
        let mut w = [0u32; 80];
        for (i, word) in chunk.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (i, &wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5A827999),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let t = a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = t;
        }
        for (hi, v) in h.iter_mut().zip([a, b, c, d, e]) {
            *hi = hi.wrapping_add(v);
        }
    }
    let mut out = [0u8; 20];
    for (i, v) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&v.to_be_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn encode64(data: &[u8]) -> String {
        const ABC: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in data.chunks(3) {
            let n = chunk.iter().enumerate().fold(0u32, |acc, (i, &b)| acc | u32::from(b) << (16 - 8 * i));
            for i in 0..=chunk.len() {
                out.push(ABC[(n >> (18 - 6 * i) & 63) as usize] as char);
            }
        }
        while out.len() % 4 != 0 {
            out.push('=');
        }
        out
    }

    #[test]
    fn sha1_matches_the_known_digests() {
        assert_eq!(hex(&sha1(b"abc")), "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(hex(&sha1(b"")), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        let long = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
        assert_eq!(hex(&sha1(long)), "84983e441c3bd26ebaae4aa1f95129e5e54670f1");
    }

    #[test]
    fn base64_reads_padded_and_unpadded_text() {
        assert_eq!(base64("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(base64("aGVsbG8").unwrap(), b"hello");
        assert_eq!(base64(&encode64(b"any carnal pleas")).unwrap(), b"any carnal pleas");
        assert!(base64("a*b").is_none());
    }

    #[test]
    fn a_profile_names_its_skin_texture_and_model() {
        let json = r#"{"timestamp":1,"profileName":"Nolan","textures":{"SKIN":{"url":"http://textures.minecraft.net/texture/1a2b3c","metadata":{"model":"slim"}},"CAPE":{"url":"http://textures.minecraft.net/texture/ffff"}}}"#;
        let (texture, slim) = skin_of_textures(&encode64(json.as_bytes())).unwrap();
        assert!(slim);
        assert_eq!(texture, location("1a2b3c"));
        assert!(texture.starts_with("skins/") && texture.len() == 6 + 40);

        let wide = r#"{"textures":{"SKIN":{"url":"https://textures.minecraft.net/texture/abc.png?x=1"}}}"#;
        let (texture, slim) = skin_of_textures(&encode64(wide.as_bytes())).unwrap();
        assert!(!slim);
        assert_eq!(texture, location("abc"));

        let none = r#"{"textures":{"CAPE":{"url":"http://x/y"}}}"#;
        assert_eq!(skin_of_textures(&encode64(none.as_bytes())), None);
    }

    #[test]
    fn the_skin_hash_is_hashed_as_utf16_units() {
        let units: Vec<u8> = "ab".encode_utf16().flat_map(u16::to_le_bytes).collect();
        assert_eq!(units, [b'a', 0, b'b', 0]);
        assert_eq!(location("ab"), format!("skins/{}", hex(&sha1(&units))));
    }

    #[test]
    fn a_skin_message_survives_the_trip() {
        let msg = SkinMsg { name: "Nolan".into(), texture: "skins/abc".into(), slim: true };
        assert_eq!(SkinMsg::decode(&msg.encode()), Some(msg));
        assert_eq!(SkinMsg::decode(&[2, b'a', 0, b'b']), None);
        assert_eq!(SkinMsg::decode(&[0, b'a', b'b']), None);
    }
}
