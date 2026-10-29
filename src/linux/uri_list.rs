//! Files as `text/uri-list` carries them, for X11 and Wayland alike.

use std::path::{Path, PathBuf};

use smallvec::SmallVec;
use url::Url;

/// Each path's `file://` URL, every line ended by CR LF.
pub(crate) fn uri_list<'a>(paths: impl IntoIterator<Item = &'a Path>) -> String {
    paths
        .into_iter()
        .filter_map(|path| Url::from_file_path(path).ok())
        .fold(String::new(), |mut list, url| {
            list.push_str(url.as_str());
            list.push_str("\r\n");
            list
        })
}

/// A list's local files; comments and other URLs pass by.
pub(crate) fn paths_of(list: &[u8]) -> SmallVec<[PathBuf; 2]> {
    String::from_utf8_lossy(list)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| Url::parse(line).ok()?.to_file_path().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_list_names_the_files_it_was_made_of() {
        let paths = ["/tmp/a b", "/tmp/100%/é#;", "/tmp/c"].map(PathBuf::from);
        let list = uri_list(paths.iter().map(PathBuf::as_path));
        assert!(list.starts_with("file:///tmp/a%20b\r\n"), "{list}");
        assert_eq!(paths_of(list.as_bytes()).as_slice(), paths);
        let mixed = "# a comment\r\nhttps://example.com/x\r\nfile:///tmp/c\r\n";
        assert_eq!(
            paths_of(mixed.as_bytes()).as_slice(),
            [PathBuf::from("/tmp/c")]
        );
    }
}
