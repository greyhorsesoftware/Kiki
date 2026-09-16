//! Framing and message envelopes shared by the socket and plugin pipes.
//!
//! Two framings are accepted per connection, detected on the first byte:
//! - binary: `u32` little-endian length, `u8` type (0 = JSON, 1 = binary), payload
//! - text: newline-delimited JSON, one object per line (used by the QML shell,
//!   which reads a line-based stream; it never needs binary frames)

use crate::json::{self, Value};
use std::io::{self, Read, Write};

pub const MAX_JSON_FRAME: usize = 16 * 1024 * 1024;
pub const MAX_BINARY_FRAME: usize = 1024 * 1024;
pub const PROTOCOL_VERSION: u64 = 1;

#[derive(Debug)]
pub enum Frame {
    Json(Value),
    Binary(Vec<u8>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Framing {
    Binary,
    Text,
}

pub struct Reader<R: Read> {
    inner: R,
    framing: Option<Framing>,
    buf: Vec<u8>,
}

impl<R: Read> Reader<R> {
    pub fn new(inner: R) -> Self {
        Reader { inner, framing: None, buf: Vec::new() }
    }

    pub fn framing(&self) -> Option<Framing> {
        self.framing
    }

    /// Reads the next frame; `Ok(None)` at a clean end of stream.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> io::Result<Option<Frame>> {
        if self.framing.is_none() {
            let mut b = [0u8; 1];
            loop {
                if self.inner.read(&mut b)? == 0 {
                    return Ok(None);
                }
                match b[0] {
                    b' ' | b'\n' | b'\r' | b'\t' => continue,
                    b'{' => {
                        self.framing = Some(Framing::Text);
                        self.buf.push(b'{');
                        break;
                    }
                    _ => {
                        self.framing = Some(Framing::Binary);
                        self.buf.push(b[0]);
                        break;
                    }
                }
            }
        }
        match self.framing.unwrap() {
            Framing::Text => self.next_line(),
            Framing::Binary => self.next_binary(),
        }
    }

    fn next_line(&mut self) -> io::Result<Option<Frame>> {
        loop {
            if let Some(pos) = self.buf.iter().position(|&c| c == b'\n') {
                let line: Vec<u8> = self.buf.drain(..=pos).collect();
                let line = &line[..line.len() - 1];
                if line.iter().all(|c| c.is_ascii_whitespace()) {
                    continue;
                }
                let v = json::parse(line).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
                return Ok(Some(Frame::Json(v)));
            }
            if self.buf.len() > MAX_JSON_FRAME {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "line too long"));
            }
            let mut chunk = [0u8; 8192];
            let n = self.inner.read(&mut chunk)?;
            if n == 0 {
                return Ok(None);
            }
            self.buf.extend_from_slice(&chunk[..n]);
        }
    }

    fn fill(&mut self, want: usize) -> io::Result<bool> {
        while self.buf.len() < want {
            let mut chunk = [0u8; 65536];
            let n = self.inner.read(&mut chunk)?;
            if n == 0 {
                return Ok(false);
            }
            self.buf.extend_from_slice(&chunk[..n]);
        }
        Ok(true)
    }

    fn next_binary(&mut self) -> io::Result<Option<Frame>> {
        if !self.fill(5)? {
            return Ok(None);
        }
        let len = u32::from_le_bytes([self.buf[0], self.buf[1], self.buf[2], self.buf[3]]) as usize;
        let kind = self.buf[4];
        let limit = if kind == 1 { MAX_BINARY_FRAME } else { MAX_JSON_FRAME };
        if len > limit {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "frame too large"));
        }
        if !self.fill(5 + len)? {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "truncated frame"));
        }
        let payload: Vec<u8> = self.buf[5..5 + len].to_vec();
        self.buf.drain(..5 + len);
        match kind {
            0 => {
                let v = json::parse(&payload).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
                Ok(Some(Frame::Json(v)))
            }
            1 => Ok(Some(Frame::Binary(payload))),
            _ => Err(io::Error::new(io::ErrorKind::InvalidData, "unknown frame type")),
        }
    }
}

pub fn write_json<W: Write>(w: &mut W, framing: Framing, v: &Value) -> io::Result<()> {
    let mut s = json::to_string(v);
    match framing {
        Framing::Text => {
            s.push('\n');
            w.write_all(s.as_bytes())
        }
        Framing::Binary => {
            let len = s.len() as u32;
            w.write_all(&len.to_le_bytes())?;
            w.write_all(&[0u8])?;
            w.write_all(s.as_bytes())
        }
    }
}

pub fn write_binary<W: Write>(w: &mut W, bytes: &[u8]) -> io::Result<()> {
    let len = bytes.len() as u32;
    w.write_all(&len.to_le_bytes())?;
    w.write_all(&[1u8])?;
    w.write_all(bytes)
}

// ---------------------------------------------------------------- envelopes

pub struct Request {
    pub id: u64,
    pub kind: String,
    pub body: Value,
}

pub fn parse_request(v: Value) -> Result<Request, &'static str> {
    let id = v.u64_field("id").ok_or("missing id")?;
    let kind = v.str_field("type").ok_or("missing type")?.to_string();
    Ok(Request { id, kind, body: v })
}

pub fn ok(id: u64, result: Value) -> Value {
    Value::obj().u("id", id).v("ok", result).done()
}

pub fn err(id: u64, code: &str, message: impl Into<String>) -> Value {
    Value::obj().u("id", id).v("err", Value::obj().s("code", code).s("message", message).done()).done()
}

pub fn event(name: &str) -> json::Obj {
    Value::obj().s("event", name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_round_trip() {
        let mut buf = Vec::new();
        write_json(&mut buf, Framing::Binary, &Value::obj().u("id", 1).s("type", "Ping").done()).unwrap();
        write_binary(&mut buf, b"hello").unwrap();
        let mut r = Reader::new(&buf[..]);
        match r.next().unwrap().unwrap() {
            Frame::Json(v) => assert_eq!(v.str_field("type"), Some("Ping")),
            _ => panic!(),
        }
        match r.next().unwrap().unwrap() {
            Frame::Binary(b) => assert_eq!(b, b"hello"),
            _ => panic!(),
        }
        assert!(r.next().unwrap().is_none());
    }

    #[test]
    fn text_round_trip() {
        let src = b"{\"id\":1,\"type\":\"Ping\"}\n\n{\"id\":2,\"type\":\"Version\"}\n";
        let mut r = Reader::new(&src[..]);
        assert_eq!(r.framing(), None);
        let a = r.next().unwrap().unwrap();
        assert_eq!(r.framing(), Some(Framing::Text));
        let b = r.next().unwrap().unwrap();
        match (a, b) {
            (Frame::Json(a), Frame::Json(b)) => {
                assert_eq!(a.u64_field("id"), Some(1));
                assert_eq!(b.str_field("type"), Some("Version"));
            }
            _ => panic!(),
        }
    }
}
