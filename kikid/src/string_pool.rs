//! The listing string pool: every name in one contiguous buffer, plus per-entry
//! kind bytes and precomputed natural-order sort keys.

use crate::kinds::Kind;
use crate::vfs::EntryType;

#[derive(Default)]
pub struct StringPool {
    names: Vec<u8>,
    offsets: Vec<u32>,
    lens: Vec<u16>,
    types: Vec<u8>,
    kinds: Vec<u8>,
    keys: Vec<u8>,
    key_offsets: Vec<u32>,
    key_lens: Vec<u16>,
}

impl StringPool {
    pub fn with_capacity(entries: usize) -> Self {
        StringPool {
            names: Vec::with_capacity(entries * 16),
            offsets: Vec::with_capacity(entries),
            lens: Vec::with_capacity(entries),
            types: Vec::with_capacity(entries),
            kinds: Vec::with_capacity(entries),
            keys: Vec::with_capacity(entries * 16),
            key_offsets: Vec::with_capacity(entries),
            key_lens: Vec::with_capacity(entries),
        }
    }

    pub fn len(&self) -> usize {
        self.offsets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.offsets.is_empty()
    }

    pub fn push(&mut self, name: &[u8], t: EntryType) -> u32 {
        let idx = self.offsets.len() as u32;
        let name = if name.len() > u16::MAX as usize { &name[..u16::MAX as usize] } else { name };
        self.offsets.push(self.names.len() as u32);
        self.lens.push(name.len() as u16);
        self.names.extend_from_slice(name);
        self.types.push(t as u8);
        self.kinds.push(Kind::guess(t, name) as u8);
        self.key_offsets.push(self.keys.len() as u32);
        let start = self.keys.len();
        sort_key(name, &mut self.keys);
        self.key_lens.push((self.keys.len() - start).min(u16::MAX as usize) as u16);
        idx
    }

    pub fn name(&self, i: u32) -> &[u8] {
        let o = self.offsets[i as usize] as usize;
        &self.names[o..o + self.lens[i as usize] as usize]
    }

    pub fn entry_type(&self, i: u32) -> EntryType {
        match self.types[i as usize] {
            0 => EntryType::File,
            1 => EntryType::Dir,
            2 => EntryType::Link,
            3 => EntryType::Other,
            255 => EntryType::Other,
            _ => EntryType::Unknown,
        }
    }

    pub fn set_entry_type(&mut self, i: u32, t: EntryType) {
        self.types[i as usize] = t as u8;
        if self.kinds[i as usize] == Kind::File as u8 || self.kinds[i as usize] == Kind::Other as u8 || t == EntryType::Dir || t == EntryType::Link {
            let name = self.name(i).to_vec();
            self.kinds[i as usize] = Kind::guess(t, &name) as u8;
        }
    }

    pub fn kind(&self, i: u32) -> Kind {
        Kind::from_u8(self.kinds[i as usize])
    }

    pub fn key(&self, i: u32) -> &[u8] {
        let o = self.key_offsets[i as usize] as usize;
        &self.keys[o..o + self.key_lens[i as usize] as usize]
    }

    /// Case-insensitive substring match against the name.
    pub fn name_contains(&self, i: u32, needle_lower: &[u8]) -> bool {
        if needle_lower.is_empty() {
            return true;
        }
        let n = self.name(i);
        if n.len() < needle_lower.len() {
            return false;
        }
        n.windows(needle_lower.len()).any(|w| w.iter().zip(needle_lower).all(|(a, b)| a.to_ascii_lowercase() == *b))
    }

    /// Marks an entry removed; the index stays valid but views skip it.
    pub fn remove(&mut self, i: u32) {
        self.types[i as usize] = 255;
    }

    pub fn is_removed(&self, i: u32) -> bool {
        self.types[i as usize] == 255
    }

    pub fn find(&self, name: &[u8]) -> Option<u32> {
        (0..self.len() as u32).find(|&i| !self.is_removed(i) && self.name(i) == name)
    }

    pub fn bytes(&self) -> usize {
        self.names.len() + self.keys.len() + self.offsets.len() * 4 + self.key_offsets.len() * 4 + self.lens.len() * 2 + self.key_lens.len() * 2 + self.types.len() + self.kinds.len()
    }
}

/// Natural, case-folded sort key: letters lower-cased, digit runs encoded as
/// (length byte, digits) so "file2" < "file10". Leading dot names sort last
/// among equals by leaving the dot in place (0x2E sorts before letters, so
/// dot-files come first, matching `ls -a`).
pub fn sort_key(name: &[u8], out: &mut Vec<u8>) {
    let mut i = 0;
    while i < name.len() {
        let b = name[i];
        if b.is_ascii_digit() {
            let start = i;
            while i < name.len() && name[i].is_ascii_digit() {
                i += 1;
            }
            // strip leading zeros so "007" == "7" for ordering, keep at least one digit
            let mut s = start;
            while s + 1 < i && name[s] == b'0' {
                s += 1;
            }
            let run = &name[s..i];
            out.push(0x80 | (run.len().min(127) as u8));
            out.extend_from_slice(run);
        } else {
            out.push(b.to_ascii_lowercase());
            i += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(s: &str) -> Vec<u8> {
        let mut v = Vec::new();
        sort_key(s.as_bytes(), &mut v);
        v
    }

    #[test]
    fn natural_order() {
        assert!(key("file2") < key("file10"));
        assert!(key("File10") > key("file9"));
        assert!(key("a") < key("B"));
        assert!(key("img007") == key("img7"));
        assert!(key("x") < key("x1"));
    }

    #[test]
    fn pool_basics() {
        let mut p = StringPool::with_capacity(4);
        let a = p.push(b"Zeta.png", EntryType::File);
        let b = p.push(b"alpha", EntryType::Dir);
        assert_eq!(p.name(a), b"Zeta.png");
        assert_eq!(p.kind(a), Kind::Image);
        assert_eq!(p.kind(b), Kind::Folder);
        assert!(p.key(b) < p.key(a));
        assert!(p.name_contains(a, b"eta"));
        assert!(!p.name_contains(b, b"z"));
    }
}
