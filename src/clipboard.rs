use cosmic::iced::futures::SinkExt;
use cosmic::iced::{futures, Subscription};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::Duration;
use wl_clipboard_rs::paste::ClipboardType;

pub fn hash_text(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// Image payload formats Clipit can capture, store, and re-serve.
/// The iced image widget decodes exactly these (png+jpeg come from
/// libcosmic, webp+bmp+gif from the local image dependency). Animated
/// GIFs render their first frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Webp,
    Bmp,
    Gif,
}

impl ImageFormat {
    pub fn mime(self) -> &'static str {
        match self {
            ImageFormat::Png => "image/png",
            ImageFormat::Jpeg => "image/jpeg",
            ImageFormat::Webp => "image/webp",
            ImageFormat::Bmp => "image/bmp",
            ImageFormat::Gif => "image/gif",
        }
    }

    pub fn ext(self) -> &'static str {
        match self {
            ImageFormat::Png => "png",
            ImageFormat::Jpeg => "jpg",
            ImageFormat::Webp => "webp",
            ImageFormat::Bmp => "bmp",
            ImageFormat::Gif => "gif",
        }
    }
}

/// Sniffs the actual image format from magic bytes. Clipboard peers can
/// mislabel offers, so stored bytes are trusted over announced MIME types.
pub fn sniff_format(bytes: &[u8]) -> Option<ImageFormat> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) {
        Some(ImageFormat::Png)
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some(ImageFormat::Jpeg)
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && bytes[8..12] == *b"WEBP" {
        Some(ImageFormat::Webp)
    } else if bytes.starts_with(b"GIF8") {
        Some(ImageFormat::Gif)
    } else if bytes.starts_with(b"BM") {
        Some(ImageFormat::Bmp)
    } else {
        None
    }
}

/// Content read from the clipboard.
#[derive(Debug, Clone)]
pub enum Clip {
    Text {
        text: String,
        html: Option<String>,
    },
    Image {
        bytes: Vec<u8>,
        format: ImageFormat,
    },
}

/// Reads plain text from the regular clipboard.
fn read_plain() -> Option<String> {
    read_text_from(ClipboardType::Regular)
}

/// Reads plain text from the primary selection (middle-click copy).
fn read_primary_text() -> Option<String> {
    read_text_from(ClipboardType::Primary)
}

fn read_text_from(clipboard: ClipboardType) -> Option<String> {
    use std::io::Read as _;
    use wl_clipboard_rs::paste::{get_contents, MimeType, Seat};

    let (pipe, _) = get_contents(clipboard, Seat::Unspecified, MimeType::Text).ok()?;
    let mut buf = Vec::new();
    let limit = (crate::history::MAX_ENTRY_BYTES + 1) as u64;
    pipe.take(limit).read_to_end(&mut buf).ok()?;
    if buf.len() > crate::history::MAX_ENTRY_BYTES {
        return None;
    }
    String::from_utf8(buf).ok()
}

/// Reads text/html from the regular clipboard, if offered.
fn read_html() -> Option<String> {
    use std::io::Read as _;
    use wl_clipboard_rs::paste::{get_contents, ClipboardType, MimeType, Seat};

    let (pipe, _) = get_contents(
        ClipboardType::Regular,
        Seat::Unspecified,
        MimeType::Specific("text/html"),
    )
    .ok()?;
    let mut buf = Vec::new();
    let limit = (crate::history::MAX_ENTRY_BYTES + 1) as u64;
    pipe.take(limit).read_to_end(&mut buf).ok()?;
    if buf.len() > crate::history::MAX_ENTRY_BYTES {
        return None;
    }
    String::from_utf8(buf).ok()
}

/// Reads an image from the regular clipboard. PNG is preferred, then the
/// other formats the image widget can render. Bytes must survive the
/// format sniff, so mislabeled offers are not stored.
fn read_image() -> Option<Clip> {
    use std::io::Read as _;
    use wl_clipboard_rs::paste::{get_contents, ClipboardType, Error, MimeType, Seat};

    let mimes = [
        "image/png", "image/jpeg", "image/gif", "image/webp", "image/bmp",
    ];
    for mime in mimes {
        let (pipe, _) = match get_contents(
            ClipboardType::Regular,
            Seat::Unspecified,
            MimeType::Specific(mime),
        ) {
            Ok(pair) => pair,
            Err(Error::NoMimeType) => continue,
            Err(_) => return None,
        };
        let mut buf = Vec::new();
        let limit = (crate::history::MAX_IMAGE_BYTES + 1) as u64;
        pipe.take(limit).read_to_end(&mut buf).ok()?;
        if buf.is_empty() || buf.len() > crate::history::MAX_IMAGE_BYTES {
            return None;
        }
        if let Some(actual) = sniff_format(&buf) {
            return Some(Clip::Image {
                bytes: buf,
                format: actual,
            });
        }
        return None;
    }
    None
}

/// Reads text, or (when enabled) an image, from the regular clipboard.
fn read_clipboard(capture_images: bool) -> Option<Clip> {
    match read_plain() {
        Some(text) => Some(Clip::Text { text, html: None }),
        None if capture_images => read_image(),
        None => None,
    }
}

/// Places text or image bytes on the regular clipboard, and mirrors them
/// onto the primary selection so middle-click paste uses the same content.
/// The copy helper serves the selection in a detached child, so the content
/// survives this process exiting. Rich text entries offer both text/plain
/// and text/html.
pub fn set_clipboard(content: &Clip) {
    use wl_clipboard_rs::copy::ClipboardType;

    if let Err(why) = copy_to(ClipboardType::Regular, content) {
        eprintln!("clipit: cannot set clipboard: {why}");
    }
    if let Err(why) = copy_to(ClipboardType::Primary, content) {
        eprintln!("clipit: cannot set primary selection: {why}");
    }
}

fn copy_to(
    clipboard: wl_clipboard_rs::copy::ClipboardType,
    content: &Clip,
) -> Result<(), wl_clipboard_rs::copy::Error> {
    use wl_clipboard_rs::copy::{MimeType, MimeSource, Options, Source};

    let mut options = Options::new();
    options.clipboard(clipboard);
    match content {
        Clip::Text { text, html } => match html {
            Some(html) => options.copy_multi(vec![
                MimeSource {
                    source: Source::Bytes(text.clone().into_bytes().into_boxed_slice()),
                    mime_type: MimeType::Text,
                },
                MimeSource {
                    source: Source::Bytes(html.clone().into_bytes().into_boxed_slice()),
                    mime_type: MimeType::Specific("text/html".to_string()),
                },
            ]),
            None => options.copy(
                Source::Bytes(text.clone().into_bytes().into_boxed_slice()),
                MimeType::Text,
            ),
        },
        Clip::Image { bytes, format } => options.copy(
            Source::Bytes(bytes.clone().into_boxed_slice()),
            MimeType::Specific(format.mime().to_string()),
        ),
    }
}

/// Emits new clipboard content every time it changes, and (when enabled)
/// primary-selection text. The first read after startup establishes the
/// baseline and is not emitted: an empty startup state means the first copy
/// after launch is recorded, while an existing clipboard value is not
/// re-added to the restored history.
pub fn watch(poll_ms: u64, capture_images: bool, capture_primary: bool) -> Subscription<Clip> {
    Subscription::run_with(
        (poll_ms, capture_images, capture_primary),
        |params| {
            let (poll_ms, capture_images, capture_primary) = *params;
            cosmic::iced::stream::channel(
                4,
                move |mut output: futures::channel::mpsc::Sender<Clip>| async move {
                    let mut ticker =
                        tokio::time::interval(Duration::from_millis(poll_ms.max(100)));
                    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                    // 0 means "no content on the clipboard".
                    let mut baseline_regular: u64 = 0;
                    let mut baseline_primary: u64 = 0;
                    let mut initialized = false;

                    loop {
                        ticker.tick().await;
                        let content = tokio::task::spawn_blocking(move || {
                            read_clipboard(capture_images)
                        })
                        .await
                        .ok()
                        .flatten();
                        let primary = if capture_primary {
                            tokio::task::spawn_blocking(read_primary_text)
                                .await
                                .ok()
                                .flatten()
                        } else {
                            None
                        };
                        let current_regular = match &content {
                            Some(Clip::Text { text, .. }) => {
                                // Mix kind into the hash so a text and an
                                // image with the same hash cannot collide
                                // paths.
                                hash_text(text)
                            }
                            Some(Clip::Image { bytes, .. }) => hash_bytes(bytes),
                            None => 0,
                        };
                        let current_primary =
                            primary.as_deref().map_or(0, hash_text);
                        if !initialized {
                            initialized = true;
                            baseline_regular = current_regular;
                            baseline_primary = current_primary;
                            continue;
                        }
                        if current_regular != baseline_regular {
                            baseline_regular = current_regular;
                            baseline_primary = current_primary;
                            // New content only: fetch the HTML alternative
                            // now so steady-state polls stay single-read.
                            let content = match content {
                                Some(Clip::Text { text, .. }) => {
                                    let html = tokio::task::spawn_blocking(read_html)
                                        .await
                                        .ok()
                                        .flatten();
                                    Some(Clip::Text { text, html })
                                }
                                other => other,
                            };
                            if let Some(content) = content
                                && output.send(content).await.is_err()
                            {
                                break;
                            }
                            continue;
                        }
                        if current_primary != baseline_primary {
                            baseline_primary = current_primary;
                            if let Some(text) = primary
                                && output
                                    .send(Clip::Text { text, html: None })
                                    .await
                                    .is_err()
                            {
                                break;
                            }
                        }
                    }
                },
            )
        },
    )
}

pub(crate) fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniff_detects_all_supported_formats() {
        let png = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0];
        let jpeg = [0xff, 0xd8, 0xff, 0xe0, 0, 0];
        let mut webp = b"RIFF\x00\x00\x00\x00".to_vec();
        webp.extend_from_slice(b"WEBP");
        let bmp = b"BM\x00\x00".to_vec();
        let gif = b"GIF89a\x00\x00".to_vec();

        assert_eq!(sniff_format(&png), Some(ImageFormat::Png));
        assert_eq!(sniff_format(&jpeg), Some(ImageFormat::Jpeg));
        assert_eq!(sniff_format(&webp), Some(ImageFormat::Webp));
        assert_eq!(sniff_format(&bmp), Some(ImageFormat::Bmp));
        assert_eq!(sniff_format(&gif), Some(ImageFormat::Gif));
        assert_eq!(sniff_format(b"plain text"), None);
        assert_eq!(sniff_format(&[]), None);
    }

    #[test]
    fn formats_map_to_mime_and_ext() {
        for (format, mime, ext) in [
            (ImageFormat::Png, "image/png", "png"),
            (ImageFormat::Jpeg, "image/jpeg", "jpg"),
            (ImageFormat::Webp, "image/webp", "webp"),
            (ImageFormat::Bmp, "image/bmp", "bmp"),
            (ImageFormat::Gif, "image/gif", "gif"),
        ] {
            assert_eq!(format.mime(), mime);
            assert_eq!(format.ext(), ext);
        }
    }
}
