use base64::{Engine, engine::general_purpose::STANDARD};
use std::fmt::Write;

pub const MAX_IMAGE: usize = 512 * 1024;
pub const MAX_DRAFT_IMAGES: usize = 2 * 1024 * 1024;
pub const SLOTS: usize = 5;
pub type Images = [Option<Image>; SLOTS];

#[derive(Clone)]
pub struct Image {
    source: String,
    size: usize,
    pub alt: String,
}

impl Image {
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() > MAX_IMAGE {
            return None;
        }
        let mime = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            "png"
        } else if bytes.starts_with(b"\xff\xd8\xff") {
            "jpeg"
        } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
            "webp"
        } else {
            return None;
        };
        Some(Self {
            source: format!("data:image/{mime};base64,{}", STANDARD.encode(bytes)),
            size: bytes.len(),
            alt: String::new(),
        })
    }

    pub fn from_source(source: &str) -> Option<Self> {
        let (_, encoded) = source.split_once(";base64,")?;
        if encoded.len() > MAX_IMAGE.div_ceil(3) * 4 {
            return None;
        }
        let image = Self::from_bytes(&STANDARD.decode(encoded).ok()?)?;
        (image.source == source).then_some(image)
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn render(&self, body: &mut String) {
        write!(
            body,
            r#"<img class="quiz-image" src="{}" alt="{}">"#,
            self.source,
            crate::web::escape(&self.alt)
        )
        .unwrap();
    }
}
