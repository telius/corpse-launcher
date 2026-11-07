use std::collections::HashMap;

#[derive(Clone, Debug)]
pub enum VdfValue {
    Str(String),
    Int(i64),
    Map(HashMap<String, VdfValue>),
}

impl VdfValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            VdfValue::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            VdfValue::Int(i) => Some(*i),
            VdfValue::Str(s) => s.parse::<i64>().ok(),
            _ => None,
        }
    }

    pub fn as_map(&self) -> Option<&HashMap<String, VdfValue>> {
        match self {
            VdfValue::Map(m) => Some(m),
            _ => None,
        }
    }

    pub fn get_ci(&self, key: &str) -> Option<&VdfValue> {
        let map = self.as_map()?;
        if let Some(v) = map.get(key) {
            return Some(v);
        }
        let low = key.to_lowercase();
        for (k, v) in map.iter() {
            if k.to_lowercase() == low {
                return Some(v);
            }
        }
        None
    }
}

#[derive(Debug, PartialEq)]
enum Token {
    Str(String),
    LBrace,
    RBrace,
}

fn tokenize_text_vdf(input: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let bytes = input.as_bytes();
    let n = bytes.len();
    let mut i = 0;

    while i < n {
        let b = bytes[i];
        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if b == b'/' && i + 1 < n && bytes[i + 1] == b'/' {
            i += 2;
            while i < n && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if b == b'{' {
            tokens.push(Token::LBrace);
            i += 1;
            continue;
        }
        if b == b'}' {
            tokens.push(Token::RBrace);
            i += 1;
            continue;
        }
        if b == b'"' {
            i += 1;
            let mut s = Vec::new();
            while i < n {
                if bytes[i] == b'\\' && i + 1 < n {
                    i += 1;
                    match bytes[i] {
                        b'n' => s.push(b'\n'),
                        b'r' => s.push(b'\r'),
                        b't' => s.push(b'\t'),
                        b'\\' => s.push(b'\\'),
                        b'"' => s.push(b'"'),
                        other => s.push(other),
                    }
                    i += 1;
                } else if bytes[i] == b'"' {
                    i += 1;
                    break;
                } else {
                    s.push(bytes[i]);
                    i += 1;
                }
            }
            tokens.push(Token::Str(String::from_utf8_lossy(&s).into_owned()));
            continue;
        }
        // Unquoted string token
        let start = i;
        while i < n
            && !bytes[i].is_ascii_whitespace()
            && bytes[i] != b'{'
            && bytes[i] != b'}'
            && bytes[i] != b'"'
        {
            i += 1;
        }
        if i > start {
            tokens.push(Token::Str(
                String::from_utf8_lossy(&bytes[start..i]).into_owned(),
            ));
        } else {
            i += 1;
        }
    }
    tokens
}

fn parse_map(tokens: &[Token], pos: &mut usize) -> HashMap<String, VdfValue> {
    let mut map = HashMap::new();
    while *pos < tokens.len() {
        match &tokens[*pos] {
            Token::RBrace => {
                *pos += 1;
                break;
            }
            Token::Str(key) => {
                let k = key.clone();
                *pos += 1;
                if *pos >= tokens.len() {
                    break;
                }
                match &tokens[*pos] {
                    Token::LBrace => {
                        *pos += 1;
                        let child = parse_map(tokens, pos);
                        // Merge if duplicate key exists and both are maps
                        if let Some(VdfValue::Map(existing)) = map.get_mut(&k) {
                            for (ck, cv) in child {
                                existing.insert(ck, cv);
                            }
                        } else {
                            map.insert(k, VdfValue::Map(child));
                        }
                    }
                    Token::Str(val) => {
                        let v = val.clone();
                        *pos += 1;
                        map.insert(k, VdfValue::Str(v));
                    }
                    Token::RBrace => {
                        break;
                    }
                }
            }
            Token::LBrace => {
                *pos += 1;
                let _ = parse_map(tokens, pos);
            }
        }
    }
    map
}

pub fn parse_text(input: &str) -> VdfValue {
    let tokens = tokenize_text_vdf(input);
    let mut pos = 0;
    VdfValue::Map(parse_map(&tokens, &mut pos))
}

fn read_cstr(bytes: &[u8], pos: &mut usize) -> String {
    let start = *pos;
    while *pos < bytes.len() && bytes[*pos] != 0 {
        *pos += 1;
    }
    let s = String::from_utf8_lossy(&bytes[start..*pos]).into_owned();
    if *pos < bytes.len() {
        *pos += 1; // skip null terminator
    }
    s
}

fn parse_binary_map(bytes: &[u8], pos: &mut usize) -> HashMap<String, VdfValue> {
    let mut map = HashMap::new();
    while *pos < bytes.len() {
        let typ = bytes[*pos];
        *pos += 1;
        if typ == 0x08 || typ == 0x0b {
            break;
        }
        let key = read_cstr(bytes, pos);
        match typ {
            0x00 => {
                let child = parse_binary_map(bytes, pos);
                map.insert(key, VdfValue::Map(child));
            }
            0x01 => {
                let val = read_cstr(bytes, pos);
                map.insert(key, VdfValue::Str(val));
            }
            0x02 | 0x07 => {
                if *pos + 4 <= bytes.len() {
                    let mut arr = [0u8; 4];
                    arr.copy_from_slice(&bytes[*pos..*pos + 4]);
                    *pos += 4;
                    let v = u32::from_le_bytes(arr) as i64;
                    map.insert(key, VdfValue::Int(v));
                } else {
                    break;
                }
            }
            0x03 => {
                // float32
                if *pos + 4 <= bytes.len() {
                    *pos += 4;
                } else {
                    break;
                }
            }
            0x0a => {
                // int64
                if *pos + 8 <= bytes.len() {
                    let mut arr = [0u8; 8];
                    arr.copy_from_slice(&bytes[*pos..*pos + 8]);
                    *pos += 8;
                    let v = i64::from_le_bytes(arr);
                    map.insert(key, VdfValue::Int(v));
                } else {
                    break;
                }
            }
            _ => {
                break;
            }
        }
    }
    map
}

pub fn parse_binary(bytes: &[u8]) -> VdfValue {
    let mut pos = 0;
    VdfValue::Map(parse_binary_map(bytes, &mut pos))
}
