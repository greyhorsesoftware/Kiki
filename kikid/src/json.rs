//! Minimal JSON reader and writer for the kiki protocols.
//!
//! The grammar is exactly what the protocol uses: objects, arrays, strings,
//! integers, floats, booleans and null. The reader never panics on bad input;
//! it returns `Err` with a byte offset. The writer produces compact output.

use std::collections::BTreeMap;
use std::fmt::Write as _;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Uint(u64),
    Float(f64),
    Str(String),
    Arr(Vec<Value>),
    Obj(BTreeMap<String, Value>),
}

#[derive(Debug)]
pub struct ParseError {
    pub offset: usize,
    pub message: &'static str,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "json error at byte {}: {}", self.offset, self.message)
    }
}

impl std::error::Error for ParseError {}

const MAX_DEPTH: usize = 64;

// ---------------------------------------------------------------- reading

pub fn parse(input: &[u8]) -> Result<Value, ParseError> {
    let mut p = Parser { s: input, i: 0 };
    p.ws();
    let v = p.value(0)?;
    p.ws();
    if p.i != p.s.len() {
        return Err(p.err("trailing characters"));
    }
    Ok(v)
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl<'a> Parser<'a> {
    fn err(&self, message: &'static str) -> ParseError {
        ParseError { offset: self.i, message }
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn ws(&mut self) {
        while let Some(c) = self.peek() {
            if c == b' ' || c == b'\n' || c == b'\r' || c == b'\t' {
                self.i += 1;
            } else {
                break;
            }
        }
    }

    fn expect(&mut self, c: u8) -> Result<(), ParseError> {
        if self.peek() == Some(c) {
            self.i += 1;
            Ok(())
        } else {
            Err(self.err("unexpected character"))
        }
    }

    fn value(&mut self, depth: usize) -> Result<Value, ParseError> {
        if depth > MAX_DEPTH {
            return Err(self.err("nesting too deep"));
        }
        match self.peek() {
            None => Err(self.err("unexpected end")),
            Some(b'{') => self.object(depth),
            Some(b'[') => self.array(depth),
            Some(b'"') => Ok(Value::Str(self.string()?)),
            Some(b't') => self.literal(b"true", Value::Bool(true)),
            Some(b'f') => self.literal(b"false", Value::Bool(false)),
            Some(b'n') => self.literal(b"null", Value::Null),
            Some(c) if c == b'-' || c.is_ascii_digit() => self.number(),
            Some(_) => Err(self.err("unexpected character")),
        }
    }

    fn literal(&mut self, word: &[u8], v: Value) -> Result<Value, ParseError> {
        if self.s[self.i..].starts_with(word) {
            self.i += word.len();
            Ok(v)
        } else {
            Err(self.err("bad literal"))
        }
    }

    fn object(&mut self, depth: usize) -> Result<Value, ParseError> {
        self.expect(b'{')?;
        let mut m = BTreeMap::new();
        self.ws();
        if self.peek() == Some(b'}') {
            self.i += 1;
            return Ok(Value::Obj(m));
        }
        loop {
            self.ws();
            if self.peek() != Some(b'"') {
                return Err(self.err("expected key"));
            }
            let k = self.string()?;
            self.ws();
            self.expect(b':')?;
            self.ws();
            let v = self.value(depth + 1)?;
            m.insert(k, v);
            self.ws();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b'}') => {
                    self.i += 1;
                    return Ok(Value::Obj(m));
                }
                _ => return Err(self.err("expected , or }")),
            }
        }
    }

    fn array(&mut self, depth: usize) -> Result<Value, ParseError> {
        self.expect(b'[')?;
        let mut a = Vec::new();
        self.ws();
        if self.peek() == Some(b']') {
            self.i += 1;
            return Ok(Value::Arr(a));
        }
        loop {
            self.ws();
            a.push(self.value(depth + 1)?);
            self.ws();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b']') => {
                    self.i += 1;
                    return Ok(Value::Arr(a));
                }
                _ => return Err(self.err("expected , or ]")),
            }
        }
    }

    fn number(&mut self) -> Result<Value, ParseError> {
        let start = self.i;
        let mut is_float = false;
        if self.peek() == Some(b'-') {
            self.i += 1;
        }
        while let Some(c) = self.peek() {
            match c {
                b'0'..=b'9' => self.i += 1,
                b'.' | b'e' | b'E' | b'+' | b'-' => {
                    is_float = true;
                    self.i += 1;
                }
                _ => break,
            }
        }
        let text = std::str::from_utf8(&self.s[start..self.i]).map_err(|_| self.err("bad number"))?;
        if text == "-" || text.is_empty() {
            return Err(self.err("bad number"));
        }
        if !is_float {
            if let Ok(u) = text.parse::<u64>() {
                return Ok(Value::Uint(u));
            }
            if let Ok(i) = text.parse::<i64>() {
                return Ok(Value::Int(i));
            }
        }
        text.parse::<f64>()
            .map(Value::Float)
            .map_err(|_| self.err("bad number"))
    }

    fn string(&mut self) -> Result<String, ParseError> {
        self.expect(b'"')?;
        let mut out: Vec<u8> = Vec::new();
        loop {
            let c = self.peek().ok_or_else(|| self.err("unterminated string"))?;
            self.i += 1;
            match c {
                b'"' => break,
                b'\\' => {
                    let e = self.peek().ok_or_else(|| self.err("unterminated escape"))?;
                    self.i += 1;
                    match e {
                        b'"' => out.push(b'"'),
                        b'\\' => out.push(b'\\'),
                        b'/' => out.push(b'/'),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'u' => {
                            let mut cp = self.hex4()?;
                            if (0xD800..0xDC00).contains(&cp) {
                                // surrogate pair
                                if self.s[self.i..].starts_with(b"\\u") {
                                    self.i += 2;
                                    let lo = self.hex4()?;
                                    if (0xDC00..0xE000).contains(&lo) {
                                        cp = 0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00);
                                    } else {
                                        return Err(self.err("bad surrogate"));
                                    }
                                } else {
                                    return Err(self.err("bad surrogate"));
                                }
                            }
                            let ch = char::from_u32(cp).ok_or_else(|| self.err("bad code point"))?;
                            let mut buf = [0u8; 4];
                            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                        }
                        _ => return Err(self.err("bad escape")),
                    }
                }
                c if c < 0x20 => return Err(self.err("control character in string")),
                c => out.push(c),
            }
        }
        String::from_utf8(out).map_err(|_| self.err("invalid utf-8"))
    }

    fn hex4(&mut self) -> Result<u32, ParseError> {
        if self.i + 4 > self.s.len() {
            return Err(self.err("short unicode escape"));
        }
        let mut v = 0u32;
        for k in 0..4 {
            let c = self.s[self.i + k];
            let d = match c {
                b'0'..=b'9' => c - b'0',
                b'a'..=b'f' => c - b'a' + 10,
                b'A'..=b'F' => c - b'A' + 10,
                _ => return Err(self.err("bad hex")),
            } as u32;
            v = (v << 4) | d;
        }
        self.i += 4;
        Ok(v)
    }
}

// ---------------------------------------------------------------- writing

pub fn write(v: &Value, out: &mut String) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Int(i) => {
            let _ = write!(out, "{i}");
        }
        Value::Uint(u) => {
            let _ = write!(out, "{u}");
        }
        Value::Float(f) => {
            if f.is_finite() {
                let _ = write!(out, "{f}");
            } else {
                out.push_str("null");
            }
        }
        Value::Str(s) => write_str(s, out),
        Value::Arr(a) => {
            out.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write(x, out);
            }
            out.push(']');
        }
        Value::Obj(m) => {
            out.push('{');
            for (i, (k, x)) in m.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_str(k, out);
                out.push(':');
                write(x, out);
            }
            out.push('}');
        }
    }
}

pub fn write_str(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

pub fn to_string(v: &Value) -> String {
    let mut s = String::new();
    write(v, &mut s);
    s
}

// ---------------------------------------------------------------- builders and accessors

impl Value {
    pub fn obj() -> Obj {
        Obj(BTreeMap::new())
    }
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Obj(m) => m.get(key),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Value::Uint(u) => Some(*u),
            Value::Int(i) if *i >= 0 => Some(*i as u64),
            Value::Float(f) if *f >= 0.0 && f.fract() == 0.0 => Some(*f as u64),
            _ => None,
        }
    }
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            Value::Uint(u) if *u <= i64::MAX as u64 => Some(*u as i64),
            _ => None,
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }
    pub fn as_arr(&self) -> Option<&[Value]> {
        match self {
            Value::Arr(a) => Some(a),
            _ => None,
        }
    }
    pub fn str_field(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(Value::as_str)
    }
    pub fn u64_field(&self, key: &str) -> Option<u64> {
        self.get(key).and_then(Value::as_u64)
    }
}

/// Small builder so call sites read as `Value::obj().s("type","Rows").u("n",3).done()`.
pub struct Obj(BTreeMap<String, Value>);

impl Obj {
    pub fn s(mut self, k: &str, v: impl Into<String>) -> Self {
        self.0.insert(k.to_string(), Value::Str(v.into()));
        self
    }
    pub fn u(mut self, k: &str, v: u64) -> Self {
        self.0.insert(k.to_string(), Value::Uint(v));
        self
    }
    pub fn i(mut self, k: &str, v: i64) -> Self {
        self.0.insert(k.to_string(), Value::Int(v));
        self
    }
    pub fn b(mut self, k: &str, v: bool) -> Self {
        self.0.insert(k.to_string(), Value::Bool(v));
        self
    }
    pub fn v(mut self, k: &str, v: Value) -> Self {
        self.0.insert(k.to_string(), v);
        self
    }
    pub fn opt_s(mut self, k: &str, v: Option<&str>) -> Self {
        self.0.insert(
            k.to_string(),
            v.map(|s| Value::Str(s.to_string())).unwrap_or(Value::Null),
        );
        self
    }
    pub fn done(self) -> Value {
        Value::Obj(self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let src = r#"{"a":[1,-2,3.5,"x\ny",true,null],"b":{"c":"\u00e9\ud83d\ude00"}}"#;
        let v = parse(src.as_bytes()).unwrap();
        let out = to_string(&v);
        let v2 = parse(out.as_bytes()).unwrap();
        assert_eq!(v, v2);
        assert_eq!(v.get("b").unwrap().str_field("c"), Some("é😀"));
    }

    #[test]
    fn rejects_garbage_without_panic() {
        for bad in ["", "{", "[1,", "\"abc", "{\"a\" 1}", "01x", "\"\\q\"", "nul", "{\"a\":\"\u{1}\"}"] {
            assert!(parse(bad.as_bytes()).is_err(), "{bad:?}");
        }
        let deep = "[".repeat(200);
        assert!(parse(deep.as_bytes()).is_err());
    }

    #[test]
    fn numbers() {
        assert_eq!(parse(b"18446744073709551615").unwrap(), Value::Uint(u64::MAX));
        assert_eq!(parse(b"-5").unwrap(), Value::Int(-5));
        assert_eq!(parse(b"1e3").unwrap(), Value::Float(1000.0));
    }
}
