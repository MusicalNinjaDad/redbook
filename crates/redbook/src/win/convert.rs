//! Conversion wrappers around windows ideosyncracies

use std::{
    fmt::{Debug, Display},
    path::PathBuf,
};

use crate::Frame;

use super::sys::{GUID, PCWSTR};

/// A windows GUID - e.g. `53F56308-B6BF-11D0-94F2-00A0C91EFB8B`
#[derive(Clone, Copy)]
pub struct Guid(pub GUID);

impl PartialEq for Guid {
    fn eq(&self, other: &Self) -> bool {
        self.0.data1 == other.0.data1
            && self.0.data2 == other.0.data2
            && self.0.data3 == other.0.data3
            && self.0.data4 == other.0.data4
    }
}

impl Eq for Guid {}

impl Debug for Guid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Guid")
            .field(&format_args!("{self}"))
            .finish()
    }
}

impl Display for Guid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let guid = self.0;

        let mut data4 = [0; 2];
        data4.copy_from_slice(&guid.data4[0..=1]);

        let mut data5 = [0; 8];
        data5[2..].copy_from_slice(&guid.data4[2..]);

        write!(
            f,
            "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
            guid.data1,
            guid.data2,
            guid.data3,
            u16::from_be_bytes(data4),
            u64::from_be_bytes(data5)
        )
    }
}

/// A pseudo-sector on an AudioCd
///
/// Windows DeviceIoControl wants offsets which pretend a [FRAME_SIZE][crate::FRAME_SIZE]-byte
/// frame is a 2048-byte sector.
///
/// Internally stores the relative frame (excluding 150 lead-in frames)
pub struct Sector(i64);

impl Sector {
    /// Construct from an absolute frame number (including lead-in)
    pub fn from_frame(frame: Frame) -> Self {
        Self(frame.relative_to_leadin().as_usize() as i64)
    }

    /// For passing to `DeviceIoControl(..,IOCTL_CDROM_RAW_READ,..)`
    ///
    /// - Pretends that each frame is a 2048-byte sector.
    /// - Returned offset is relative to start of audio data
    pub fn offset(&self) -> i64 {
        self.0 * 2048
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// A somewhat sane way of dealing with `PWSTR/PCWSTR`: A pointer to a null terminated string
/// consisting of 'wide chars' (u16), encoded using UTF-16.
///
/// Construct via `WinString::from(&str)`
pub struct WinString {
    words: Vec<u16>,
}

impl From<&str> for WinString {
    fn from(utf8: &str) -> Self {
        // see https://kennykerr.ca/rust-getting-started/string-tutorial.html
        let words = utf8.encode_utf16().chain(Some(0)).collect();
        Self { words }
    }
}

impl From<String> for WinString {
    fn from(utf8: String) -> Self {
        utf8.as_str().into()
    }
}

impl From<&[u16]> for WinString {
    /// From a NULL-terminated series of u16 as used by windows ffi
    ///
    /// Concatenates after first null-byte
    ///
    /// TODO: validate is valid UTF-16
    fn from(bytes: &[u16]) -> Self {
        let words = bytes
            .iter()
            .take_while(|c| **c != 0)
            .copied()
            // We've stripped the the termination with take_while, so add it back
            .chain(Some(0))
            .collect();
        Self { words }
    }
}

impl Display for WinString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(
            &String::from_utf16_lossy(&self.words[..self.words.len() - 1]),
            f,
        )
    }
}

#[expect(
    rustdoc::private_intra_doc_links,
    reason = "1. in-IDE help, 2. lint enforcement of privacy"
)]
impl WinString {
    /// Create a [`PCWSTR`] - note this is a raw pointer.
    ///
    /// You must ensure that the returned [`PCWSTR`] is not used after self is dropped.
    pub fn as_pcwstr(&self) -> PCWSTR {
        self.words.as_ptr()
    }

    /// A slice of `u16` "words" representing the stored characters.
    /// Including the final `\0` termination.
    pub fn as_words(&self) -> &[u16] {
        &self.words
    }
}

/// A Windows path either in [traditional] format as a [PathBuf], or [Dos Device Path][dosdevicepath]
/// format as a [WinString].
///
/// More information is available on MS-Learn about [path formats][pathformats], [normalisation] &
/// [long paths][longpath]
///
/// [pathformats]: https://learn.microsoft.com/en-us/dotnet/standard/io/file-path-formats
/// [traditional]: https://learn.microsoft.com/en-us/dotnet/standard/io/file-path-formats#traditional-dos-paths
/// [dosdevicepath]: https://learn.microsoft.com/en-us/dotnet/standard/io/file-path-formats#dos-device-paths
/// [normalisation]: https://learn.microsoft.com/en-us/dotnet/standard/io/file-path-formats#path-normalization
/// [longpath]: https://learn.microsoft.com/en-us/windows/win32/fileio/maximum-file-path-limitation
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WinPath {
    /// a Rust PathBuf *without* preceding `\\.\` or `\\?\`
    Traditional(PathBuf),
    /// a WinString *with* preceding `\\?\`
    DosDevicePath(WinString),
}

impl From<PathBuf> for WinPath {
    fn from(path: PathBuf) -> Self {
        let (prefix, _) = path.as_os_str().split_at(4);
        if prefix == (r"\\.\") || prefix == (r"\\?\") {
            Self::DosDevicePath(WinString::from(path.display().to_string()))
        } else {
            Self::Traditional(path)
        }
    }
}

impl WinPath {
    /// Consume the `WinPath` and provide a [WinString] representation beginning with `\\.\` or `\\?\`
    ///
    /// See [WinPath] for more details on path formats
    pub fn into_dos_device_path(self) -> WinString {
        match self {
            WinPath::Traditional(path) => WinString::from(format!(r"\\.\{}", path.display())),
            WinPath::DosDevicePath(win_string) => win_string,
        }
    }
}

#[cfg(test)]
#[forbid(unsafe_code)]
mod tests {

    use std::assert_matches;

    use super::*;

    #[test]
    /// See https://learn.microsoft.com/en-us/dotnet/api/system.guid.-ctor?view=net-10.0#system-guid-ctor(system-int32-system-int16-system-int16-system-byte())
    fn format_guid() {
        let guid = Guid(GUID {
            data1: 1,
            data2: 2,
            data3: 3,
            data4: [0, 1, 2, 3, 4, 5, 6, 7],
        });

        let expected = "00000001-0002-0003-0001-020304050607";

        assert_eq!(guid.to_string(), expected);
    }

    #[test]
    fn display_winstring() {
        let s = "a/load/of/text";
        let w = WinString::from(s);
        assert_eq!(s, w.to_string());
    }

    #[test]
    fn device_path() {
        let devpath = r"\\?\usbstor#cdrom&ven_hl-dt-st&prod_dvdram_gue1n&rev_as00#4b4d555642414e313035a920&0#{53f56308-b7bf-11d0-95f2-00a0e91efb8b}";
        let path = PathBuf::from(devpath);
        let winpath = WinPath::from(path);
        assert_matches!(winpath, WinPath::DosDevicePath(p) if p == WinString::from(devpath));
    }
}
