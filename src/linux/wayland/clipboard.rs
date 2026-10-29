use std::{
    fs::File,
    io::{ErrorKind, Write},
    os::fd::{AsRawFd, BorrowedFd, OwnedFd},
};

use calloop::{LoopHandle, PostAction};
use filedescriptor::Pipe;
use strum::IntoEnumIterator;
use wayland_client::protocol::wl_data_source::WlDataSource;
use wayland_client::{Connection, protocol::wl_data_offer::WlDataOffer};
use wayland_protocols::wp::primary_selection::zv1::client::zwp_primary_selection_offer_v1::ZwpPrimarySelectionOfferV1;
use wayland_protocols::wp::primary_selection::zv1::client::zwp_primary_selection_source_v1::ZwpPrimarySelectionSourceV1;

use crate::linux::platform::{PIPE_READ_TIMEOUT, read_fd_with_timeout};
use crate::linux::{paths_of, uri_list};
use gpui::{
    ClipboardEntry, ClipboardItem, ClipboardString, ExternalPaths, Image, ImageFormat, hash,
};

/// Text mime types that we'll offer to other programs.
pub(crate) const TEXT_MIME_TYPES: [&str; 3] =
    ["text/plain;charset=utf-8", "UTF8_STRING", "text/plain"];
pub(crate) const FILE_LIST_MIME_TYPE: &str = "text/uri-list";

/// Text mime types that we'll accept from other programs.
pub(crate) const ALLOWED_TEXT_MIME_TYPES: [&str; 2] = ["text/plain;charset=utf-8", "UTF8_STRING"];

pub(crate) struct Clipboard {
    connection: Connection,
    loop_handle: LoopHandle<'static, ()>,
    self_mime: String,

    // Internal clipboard
    contents: Option<ClipboardItem>,
    primary_contents: Option<ClipboardItem>,
    /// Whether this client set the selection, its offer still to come or here.
    claimed: bool,
    primary_claimed: bool,
    /// The sources it set; another's cancels them.
    source: Option<WlDataSource>,
    primary_source: Option<ZwpPrimarySelectionSourceV1>,

    // External clipboard
    cached_read: Option<ClipboardItem>,
    current_offer: Option<DataOffer<WlDataOffer>>,
    cached_primary_read: Option<ClipboardItem>,
    current_primary_offer: Option<DataOffer<ZwpPrimarySelectionOfferV1>>,
}

pub(crate) trait ReceiveData {
    fn receive_data(&self, mime_type: String, fd: BorrowedFd<'_>);
}

impl ReceiveData for WlDataOffer {
    fn receive_data(&self, mime_type: String, fd: BorrowedFd<'_>) {
        self.receive(mime_type, fd);
    }
}

impl ReceiveData for ZwpPrimarySelectionOfferV1 {
    fn receive_data(&self, mime_type: String, fd: BorrowedFd<'_>) {
        self.receive(mime_type, fd);
    }
}

#[derive(Clone, Debug)]
/// Wrapper for `WlDataOffer` and `ZwpPrimarySelectionOfferV1`, used to help track mime types.
pub(crate) struct DataOffer<T: ReceiveData> {
    pub inner: T,
    mime_types: Vec<String>,
}

impl<T: ReceiveData> DataOffer<T> {
    pub fn new(offer: T) -> Self {
        Self {
            inner: offer,
            mime_types: Vec::new(),
        }
    }

    pub fn add_mime_type(&mut self, mime_type: String) {
        self.mime_types.push(mime_type)
    }

    fn has_mime_type(&self, mime_type: &str) -> bool {
        self.mime_types.iter().any(|t| t == mime_type)
    }

    fn read_bytes(&self, connection: &Connection, mime_type: &str) -> Option<Vec<u8>> {
        let pipe = Pipe::new().unwrap();
        self.inner.receive_data(mime_type.to_string(), unsafe {
            BorrowedFd::borrow_raw(pipe.write.as_raw_fd())
        });
        let fd = pipe.read;
        drop(pipe.write);

        connection.flush().unwrap();

        match read_fd_with_timeout(fd, PIPE_READ_TIMEOUT) {
            Ok(bytes) => Some(bytes),
            Err(err) => {
                log::error!("error reading clipboard pipe: {err:?}");
                None
            }
        }
    }

    fn read_files(&self, connection: &Connection) -> Option<ClipboardEntry> {
        if !self.has_mime_type(FILE_LIST_MIME_TYPE) {
            return None;
        }
        let paths = paths_of(&self.read_bytes(connection, FILE_LIST_MIME_TYPE)?);
        (!paths.is_empty()).then(|| ClipboardEntry::ExternalPaths(ExternalPaths(paths)))
    }

    fn read_text(&self, connection: &Connection) -> Option<ClipboardEntry> {
        let mime_type = self.mime_types.iter().find(|&mime_type| {
            ALLOWED_TEXT_MIME_TYPES
                .iter()
                .any(|&allowed| allowed == mime_type)
        })?;
        let bytes = self.read_bytes(connection, mime_type)?;
        let text_content = match String::from_utf8(bytes) {
            Ok(content) => content,
            Err(e) => {
                log::error!("Failed to convert clipboard content to UTF-8: {}", e);
                return None;
            }
        };

        // Normalize the text to unix line endings, otherwise
        // copying from eg: firefox inserts a lot of blank
        // lines, and that is super annoying.
        let result = text_content.replace("\r\n", "\n");
        Some(ClipboardEntry::String(ClipboardString::new(result)))
    }

    fn read_image(&self, connection: &Connection) -> Option<ClipboardEntry> {
        for format in ImageFormat::iter() {
            let mime_type = format.mime_type();
            if !self.has_mime_type(mime_type) {
                continue;
            }

            if let Some(bytes) = self.read_bytes(connection, mime_type) {
                let id = hash(&bytes);
                return Some(ClipboardEntry::Image(Image { format, bytes, id }));
            }
        }
        None
    }

    /// Its files, text and first image together.
    fn read_entries(&self, connection: &Connection) -> Option<ClipboardItem> {
        let mut entries = Vec::new();
        entries.extend(self.read_files(connection));
        entries.extend(self.read_text(connection));
        entries.extend(self.read_image(connection));
        (!entries.is_empty()).then_some(ClipboardItem { entries })
    }
}

/// Whether `offer` is this client's own.
fn self_offered<T: ReceiveData>(offer: &Option<DataOffer<T>>, self_mime: &str) -> bool {
    offer
        .as_ref()
        .is_some_and(|offer| offer.has_mime_type(self_mime))
}

/// The types `item` offers other programs.
pub(crate) fn offers(item: &ClipboardItem) -> Vec<String> {
    let mut mimes: Vec<String> = Vec::new();
    for entry in &item.entries {
        let offered: &[&str] = match entry {
            ClipboardEntry::String(_) => &TEXT_MIME_TYPES,
            ClipboardEntry::Image(image) => &[image.format.mime_type()],
            ClipboardEntry::ExternalPaths(_) => &[FILE_LIST_MIME_TYPE],
        };
        for mime in offered {
            if !mimes.iter().any(|kept| kept == mime) {
                mimes.push(mime.to_string());
            }
        }
    }
    mimes
}

/// What `item` holds as `mime`, as another program asks for it.
fn bytes_for(item: &ClipboardItem, mime: &str) -> Option<Vec<u8>> {
    item.entries.iter().find_map(|entry| match entry {
        ClipboardEntry::String(_) if TEXT_MIME_TYPES.contains(&mime) => {
            item.text().map(String::into_bytes)
        }
        ClipboardEntry::Image(image) if image.format.mime_type() == mime => {
            Some(image.bytes.clone())
        }
        ClipboardEntry::ExternalPaths(paths) if mime == FILE_LIST_MIME_TYPE => {
            Some(uri_list(paths.paths().iter().map(|path| path.as_path())).into_bytes())
        }
        _ => None,
    })
}

impl Clipboard {
    pub fn new(connection: Connection, loop_handle: LoopHandle<'static, ()>) -> Self {
        Self {
            connection,
            loop_handle,
            self_mime: format!("pid/{}", std::process::id()),

            contents: None,
            primary_contents: None,
            claimed: false,
            primary_claimed: false,
            source: None,
            primary_source: None,

            cached_read: None,
            current_offer: None,
            cached_primary_read: None,
            current_primary_offer: None,
        }
    }

    /// Holds `item` as the selection this client just set.
    pub fn set(&mut self, item: ClipboardItem, source: WlDataSource) {
        self.contents = Some(item);
        self.claimed = true;
        self.source = Some(source);
    }

    /// Another client took the selection, though this one may not see its offer.
    pub fn cancelled(&mut self, source: &WlDataSource) {
        if self.source.as_ref() == Some(source) {
            self.source = None;
            self.claimed = false;
            self.set_offer(None);
        }
    }

    /// Holds `item` as the primary selection this client just set.
    pub fn set_primary(&mut self, item: ClipboardItem, source: ZwpPrimarySelectionSourceV1) {
        self.primary_contents = Some(item);
        self.primary_claimed = true;
        self.primary_source = Some(source);
    }

    /// Another client took the primary selection.
    pub fn primary_cancelled(&mut self, source: &ZwpPrimarySelectionSourceV1) {
        if self.primary_source.as_ref() == Some(source) {
            self.primary_source = None;
            self.primary_claimed = false;
            self.set_primary_offer(None);
        }
    }

    pub fn set_offer(&mut self, data_offer: Option<DataOffer<WlDataOffer>>) {
        self.cached_read = None;
        // Another client's offer, or none, ends this client's claim.
        let own = self_offered(&data_offer, &self.self_mime);
        self.claimed &= own;
        self.current_offer = data_offer;
    }

    pub fn set_primary_offer(&mut self, data_offer: Option<DataOffer<ZwpPrimarySelectionOfferV1>>) {
        self.cached_primary_read = None;
        let own = self_offered(&data_offer, &self.self_mime);
        self.primary_claimed &= own;
        self.current_primary_offer = data_offer;
    }

    pub fn self_mime(&self) -> String {
        self.self_mime.clone()
    }

    pub fn send(&self, mime_type: String, fd: OwnedFd) {
        match self
            .contents
            .as_ref()
            .and_then(|item| bytes_for(item, &mime_type))
        {
            Some(bytes) => self.send_bytes(fd, bytes),
            None => log::warn!("clipboard: asked for {mime_type}, which it does not hold"),
        }
    }

    pub fn send_primary(&self, mime_type: String, fd: OwnedFd) {
        match self
            .primary_contents
            .as_ref()
            .and_then(|item| bytes_for(item, &mime_type))
        {
            Some(bytes) => self.send_bytes(fd, bytes),
            None => log::warn!("primary selection: asked for {mime_type}, which it does not hold"),
        }
    }

    pub fn read(&mut self) -> Option<ClipboardItem> {
        // The compositor sends this client's own offer later.
        if self.claimed {
            return self.contents.clone();
        }
        let offer = self.current_offer.as_ref()?;
        if let Some(cached) = self.cached_read.clone() {
            return Some(cached);
        }

        if offer.has_mime_type(&self.self_mime) {
            return self.contents.clone();
        }

        let item = offer.read_entries(&self.connection)?;

        self.cached_read = Some(item.clone());
        Some(item)
    }

    pub fn read_primary(&mut self) -> Option<ClipboardItem> {
        if self.primary_claimed {
            return self.primary_contents.clone();
        }
        let offer = self.current_primary_offer.as_ref()?;
        if let Some(cached) = self.cached_primary_read.clone() {
            return Some(cached);
        }

        if offer.has_mime_type(&self.self_mime) {
            return self.primary_contents.clone();
        }

        let item = offer.read_entries(&self.connection)?;

        self.cached_primary_read = Some(item.clone());
        Some(item)
    }

    pub fn send_bytes(&self, fd: OwnedFd, bytes: Vec<u8>) {
        let mut written = 0;
        self.loop_handle
            .insert_source(
                calloop::generic::Generic::new(
                    File::from(fd),
                    calloop::Interest::WRITE,
                    calloop::Mode::Level,
                ),
                move |_, file, _| {
                    let file = unsafe { file.get_mut() };
                    loop {
                        match file.write(&bytes[written..]) {
                            Ok(n) if written + n == bytes.len() => {
                                written += n;
                                break Ok(PostAction::Remove);
                            }
                            Ok(n) => written += n,
                            Err(err) if err.kind() == ErrorKind::WouldBlock => {
                                break Ok(PostAction::Continue);
                            }
                            Err(_) => break Ok(PostAction::Remove),
                        }
                    }
                },
            )
            .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use gpui::{ClipboardEntry, ClipboardItem, ClipboardString, ExternalPaths, Image, ImageFormat};

    use super::{bytes_for, offers};

    #[test]
    fn an_item_offers_and_sends_each_of_its_types() {
        let item = ClipboardItem {
            entries: vec![
                ClipboardEntry::String(ClipboardString::new("two files".into())),
                ClipboardEntry::Image(Image::from_bytes(ImageFormat::Png, vec![1, 2, 3])),
                ClipboardEntry::ExternalPaths(ExternalPaths(
                    vec![PathBuf::from("/tmp/a b"), PathBuf::from("/tmp/c")].into(),
                )),
            ],
        };
        assert_eq!(
            offers(&item),
            [
                "text/plain;charset=utf-8",
                "UTF8_STRING",
                "text/plain",
                "image/png",
                "text/uri-list"
            ]
        );
        assert_eq!(bytes_for(&item, "UTF8_STRING").unwrap(), b"two files");
        assert_eq!(bytes_for(&item, "image/png").unwrap(), [1, 2, 3]);
        let list = bytes_for(&item, "text/uri-list").unwrap();
        assert_eq!(list, b"file:///tmp/a%20b\r\nfile:///tmp/c\r\n");
        assert_eq!(bytes_for(&item, "image/jpeg"), None);
    }
}
