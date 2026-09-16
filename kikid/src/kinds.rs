//! File kinds for icons, from the entry type and the extension (phase 1) or mime (phase 2).

use crate::vfs::EntryType;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Kind {
    Folder = 0,
    File = 1,
    Link = 2,
    Image = 3,
    Video = 4,
    Audio = 5,
    Document = 6,
    Pdf = 7,
    Text = 8,
    Code = 9,
    Archive = 10,
    Other = 11,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Folder => "folder",
            Kind::File => "file",
            Kind::Link => "link",
            Kind::Image => "image",
            Kind::Video => "video",
            Kind::Audio => "audio",
            Kind::Document => "document",
            Kind::Pdf => "pdf",
            Kind::Text => "text",
            Kind::Code => "code",
            Kind::Archive => "archive",
            Kind::Other => "other",
        }
    }

    pub fn from_u8(b: u8) -> Kind {
        match b {
            0 => Kind::Folder,
            1 => Kind::File,
            2 => Kind::Link,
            3 => Kind::Image,
            4 => Kind::Video,
            5 => Kind::Audio,
            6 => Kind::Document,
            7 => Kind::Pdf,
            8 => Kind::Text,
            9 => Kind::Code,
            10 => Kind::Archive,
            _ => Kind::Other,
        }
    }

    pub fn guess(t: EntryType, name: &[u8]) -> Kind {
        match t {
            EntryType::Dir => return Kind::Folder,
            EntryType::Link => return Kind::Link,
            EntryType::Other => return Kind::Other,
            EntryType::File | EntryType::Unknown => {}
        }
        let ext = match name.iter().rposition(|&b| b == b'.') {
            Some(0) | None => return Kind::File,
            Some(i) => &name[i + 1..],
        };
        let mut lower = [0u8; 8];
        if ext.is_empty() || ext.len() > 8 {
            return Kind::File;
        }
        for (i, b) in ext.iter().enumerate() {
            lower[i] = b.to_ascii_lowercase();
        }
        match &lower[..ext.len()] {
            b"png" | b"jpg" | b"jpeg" | b"gif" | b"webp" | b"avif" | b"bmp" | b"svg" | b"heic" | b"tif" | b"tiff" | b"raw" | b"cr2" | b"nef" | b"arw" | b"dng" => Kind::Image,
            b"mp4" | b"mkv" | b"webm" | b"mov" | b"avi" | b"m4v" | b"mpg" | b"mpeg" | b"ts" => Kind::Video,
            b"mp3" | b"flac" | b"ogg" | b"opus" | b"wav" | b"m4a" | b"aac" | b"aiff" => Kind::Audio,
            b"pdf" => Kind::Pdf,
            b"doc" | b"docx" | b"odt" | b"rtf" | b"xls" | b"xlsx" | b"ods" | b"ppt" | b"pptx" | b"odp" | b"epub" => Kind::Document,
            b"txt" | b"md" | b"markdown" | b"rst" | b"log" | b"csv" | b"tsv" | b"ini" | b"cfg" | b"conf" | b"nfo" => Kind::Text,
            b"rs" | b"c" | b"h" | b"cpp" | b"hpp" | b"cc" | b"go" | b"java" | b"kt" | b"js" | b"mjs" | b"jsx" | b"tsx" | b"py" | b"rb" | b"sh" | b"bash" | b"zsh" | b"fish" | b"lua" | b"nix" | b"qml" | b"toml" | b"yaml" | b"yml" | b"json" | b"html" | b"htm" | b"css" | b"scss" | b"sql" | b"php" | b"swift" | b"zig" | b"ex" | b"exs" | b"hs" | b"ml" | b"scala" | b"cs" | b"vim" | b"el" => Kind::Code,
            b"zip" | b"tar" | b"gz" | b"tgz" | b"xz" | b"zst" | b"bz2" | b"7z" | b"rar" | b"deb" | b"rpm" | b"iso" => Kind::Archive,
            _ => Kind::File,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn guesses() {
        assert_eq!(Kind::guess(EntryType::File, b"photo.JPG"), Kind::Image);
        assert_eq!(Kind::guess(EntryType::File, b"main.rs"), Kind::Code);
        assert_eq!(Kind::guess(EntryType::File, b".zshrc"), Kind::File);
        assert_eq!(Kind::guess(EntryType::File, b"archive.tar.zst"), Kind::Archive);
        assert_eq!(Kind::guess(EntryType::Dir, b"x.png"), Kind::Folder);
        assert_eq!(Kind::guess(EntryType::File, b"ts"), Kind::File);
    }
}
