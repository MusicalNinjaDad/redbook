//! Filename sanitization for Windows compatibility.
//!
//! This module provides traits for sanitizing strings and characters to be used
//! as filenames on Windows systems.
//!
//! # Sanitization Rules
//!
//! - Removes all non-ASCII characters (only ASCII code points are allowed)
//! - Removes ASCII control characters (code points 0-31 and 127)
//! - Removes Windows reserved filename characters: `< > : " / \ | ? *`
//!
//! # Notes
//!
//! This module does **not** check for reserved filenames (CON, PRN, AUX, NUL,
//! COM1-COM9, LPT1-LPT9, etc.). If you need to handle reserved filenames, you
//! should add additional validation.

/// Trait for checking if a character is valid for use in a filename.
#[cfg_attr(
    not(any(target_family = "windows", test)),
    expect(dead_code, reason = "stubs")
)]
pub trait FilenameChar {
    /// Returns `true` if the character is valid for a filename on Windows.
    ///
    /// A character is considered valid if:
    /// - It is an ASCII character (`char::is_ascii()` returns `true`)
    /// - Its Unicode code point is between 32 and 126 inclusive
    /// - It is not one of the reserved filename characters: `< > : " / \ | ? *`
    ///
    /// # Examples
    ///
    /// ```
    /// assert!('a'.is_valid_filename_char());
    /// assert!(!'<'.is_valid_filename_char());
    /// assert!(!'\x00'.is_valid_filename_char());
    /// assert!(!'é'.is_valid_filename_char());
    /// ```
    fn is_valid_filename_char(&self) -> bool;
}

impl FilenameChar for char {
    fn is_valid_filename_char(&self) -> bool {
        self.is_ascii()
            && *self >= ' '
            && *self <= '~'
            && !matches!(self, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
    }
}

/// Trait for sanitizing strings to be used as filenames.
#[cfg_attr(
    not(any(target_family = "windows", test)),
    expect(dead_code, reason = "stubs")
)]
pub trait FilenameSanitize {
    /// Returns a sanitized version of the string suitable for use as a filename.
    ///
    /// The sanitization process:
    /// 1. Removes all non-ASCII characters
    /// 2. Removes all ASCII control characters (code points 0-31 and 127)
    /// 3. Removes all Windows reserved characters: `< > : " / \ | ? *`
    ///
    /// # Examples
    ///
    /// ```
    /// assert_eq!(
    ///     "Rocket Man: The Definitive Hits".sanitize_filename(),
    ///     "Rocket Man The Definitive Hits"
    /// );
    /// assert_eq!("file?name*.txt".sanitize_filename(), "filename.txt");
    /// assert_eq!("café".sanitize_filename(), "caf");
    /// assert_eq!("\x00\x01".sanitize_filename(), "");
    /// ```
    fn sanitize_filename(&self) -> String;
}

impl FilenameSanitize for str {
    fn sanitize_filename(&self) -> String {
        self.chars()
            .filter(|c| c.is_valid_filename_char())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_specified_cases() {
        assert_eq!(
            "Rocket Man: The Definitive Hits".sanitize_filename(),
            "Rocket Man The Definitive Hits"
        );
        assert_eq!("file?name*.txt".sanitize_filename(), "filename.txt");
        assert_eq!("a<b>c".sanitize_filename(), "abc");
        assert_eq!("path/to\\file".sanitize_filename(), "pathtofile");
        assert_eq!("a|b".sanitize_filename(), "ab");
        assert_eq!("\"quoted\"".sanitize_filename(), "quoted");
    }

    #[test]
    fn test_non_ascii_removed() {
        assert_eq!("café".sanitize_filename(), "caf");
        assert_eq!("日本語".sanitize_filename(), "");
        assert_eq!("mixed文本".sanitize_filename(), "mixed");
    }

    #[test]
    fn test_control_characters_removed() {
        assert_eq!("\x00\x01\x1f".sanitize_filename(), "");
        assert_eq!("a\x07b".sanitize_filename(), "ab");
        assert_eq!("test\x7f.txt".sanitize_filename(), "test.txt");
    }

    #[test]
    fn test_empty_string() {
        assert_eq!("".sanitize_filename(), "");
    }

    #[test]
    fn test_all_valid() {
        assert_eq!(
            "valid_filename.txt".sanitize_filename(),
            "valid_filename.txt"
        );
    }

    #[test]
    fn test_all_invalid() {
        assert_eq!("<>:\"/\\|?*".sanitize_filename(), "");
    }

    #[test]
    fn test_spaces_and_dots_preserved() {
        assert_eq!("file name.txt".sanitize_filename(), "file name.txt");
        assert_eq!(".hidden".sanitize_filename(), ".hidden");
    }

    #[test]
    fn test_char_trait() {
        assert!('a'.is_valid_filename_char());
        assert!(' '.is_valid_filename_char());
        assert!('.'.is_valid_filename_char());
        assert!(!'<'.is_valid_filename_char());
        assert!(!'>'.is_valid_filename_char());
        assert!(!':'.is_valid_filename_char());
        assert!('\''.is_valid_filename_char());
        assert!(!'"'.is_valid_filename_char());
        assert!(!'/'.is_valid_filename_char());
        assert!(!'\\'.is_valid_filename_char());
        assert!(!'|'.is_valid_filename_char());
        assert!(!'?'.is_valid_filename_char());
        assert!(!'*'.is_valid_filename_char());
        assert!(!'\x00'.is_valid_filename_char());
        assert!(!'\x1f'.is_valid_filename_char());
        assert!(!'é'.is_valid_filename_char());
    }
}
