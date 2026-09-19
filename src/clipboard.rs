use cosmic::iced::futures::SinkExt;
use cosmic::iced::{futures, Subscription};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::Duration;

pub fn hash_text(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// Content read from the clipboard.
#[derive(Debug, Clone)]
pub enum Clip {
    Text(String),
    Image(Vec<u8>),
}

const PNG_MAGIC: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

/// Reads text, or (when enabled) a PNG image, from the regular clipboard.
fn read_clipboard(capture_images: bool) -> Option<Clip> {
    use std::io::Read as _;
    use wl_clipboard_rs::paste::{get_contents, ClipboardType, Error, MimeType, Seat};

    match get_contents(ClipboardType::Regular, Seat::Unspecified, MimeType::Text) {
        Ok((pipe, _)) => {
            let mut buf = Vec::new();
            let limit = (crate::history::MAX_ENTRY_BYTES + 1) as u64;
            match pipe.take(limit).read_to_end(&mut buf) {
                Ok(_) => {}
                Err(why) => {
                    eprintln!("clipit: clipboard read error: {why}");
                    return None;
                }
            }
            if buf.len() > crate::history::MAX_ENTRY_BYTES {
                return None;
            }
            String::from_utf8(buf).ok().map(Clip::Text)
        }
        Err(Error::NoMimeType) if capture_images => {
            let (pipe, _) = get_contents(
                ClipboardType::Regular,
                Seat::Unspecified,
                MimeType::Specific("image/png"),
            )
            .ok()?;
            let mut buf = Vec::new();
            let limit = (crate::history::MAX_IMAGE_BYTES + 1) as u64;
            pipe.take(limit).read_to_end(&mut buf).ok()?;
            if buf.is_empty() || buf.len() > crate::history::MAX_IMAGE_BYTES {
                return None;
            }
            // Cheap PNG signature check so random binary offers are skipped.
            if buf.starts_with(&PNG_MAGIC) {
                Some(Clip::Image(buf))
            } else {
                None
            }
        }
        Err(Error::ClipboardEmpty) | Err(Error::NoSeats) | Err(Error::NoMimeType) => None,
        Err(why) => {
            eprintln!("clipit: clipboard poll error: {why}");
            None
        }
    }
}

/// Places text or image bytes on the regular clipboard. The copy helper
/// serves the selection in a detached child, so the content survives this
/// process exiting.
pub fn set_clipboard(content: Clip) {
    use wl_clipboard_rs::copy::{MimeType, Options, Source};

    let options = Options::new();
    match content {
        Clip::Text(text) => {
            let _ = options.copy(Source::Bytes(text.into_bytes().into()), MimeType::Text);
        }
        Clip::Image(bytes) => {
            let _ = options.copy(
                Source::Bytes(bytes.into()),
                MimeType::Specific("image/png".to_string()),
            );
        }
    }
}

/// Emits new clipboard content every time it changes. The first read after
/// startup establishes the baseline and is not emitted: an empty startup
/// state means the first copy after launch is recorded, while an existing
/// clipboard value is not re-added to the restored history.
pub fn watch(poll_ms: u64, capture_images: bool) -> Subscription<Clip> {
    Subscription::run_with((poll_ms, capture_images), |params| {
        let (poll_ms, capture_images) = *params;
        cosmic::iced::stream::channel(
            4,
            move |mut output: futures::channel::mpsc::Sender<Clip>| async move {
                let mut ticker =
                    tokio::time::interval(Duration::from_millis(poll_ms.max(100)));
                ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                // 0 means "no content on the clipboard".
                let mut baseline: u64 = 0;
                let mut initialized = false;

                loop {
                    ticker.tick().await;
                    let content =
                        tokio::task::spawn_blocking(move || read_clipboard(capture_images))
                            .await
                            .ok()
                            .flatten();
                    let current = match &content {
                        Some(Clip::Text(t)) => {
                            // Mix kind into the hash so a text and an image
                            // with the same hash cannot collide paths.
                            hash_text(t)
                        }
                        Some(Clip::Image(b)) => hash_bytes(b),
                        None => 0,
                    };
                    if !initialized {
                        initialized = true;
                        baseline = current;
                        continue;
                    }
                    if current != baseline {
                        baseline = current;
                        if let Some(content) = content
                            && output.send(content).await.is_err()
                        {
                            break;
                        }
                    }
                }
            },
        )
    })
}

fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}
