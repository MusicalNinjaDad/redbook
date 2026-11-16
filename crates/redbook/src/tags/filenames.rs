/// Returns a sanitised version of the string suitable for use as a filename.
///
/// We strip out invalid / dangerous characters, based on the target, and additionally remove any
/// leading / trailing whitespace and leading `-`
pub(crate) fn sanitise(pathsegment: &str) -> String {
    #[cfg(windows)]
    /// Returns `true` if the character is valid for a filename on Windows.
    ///
    /// A character is considered valid if:
    /// - It is an ASCII character (`char::is_ascii()` returns `true`)
    /// - Its Unicode code point is between 32 and 126 inclusive
    /// - It is not one of the reserved filename characters: `< > : " / \ | ? *`
    fn is_valid(c: &char) -> bool {
        let reserved = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
        c.is_ascii() && *c >= ' ' && *c <= '~' && !reserved.contains(c)
    }

    #[cfg(unix)]
    /// Returns `true` if the character is valid for a (sane) filename on Posix.
    ///
    /// While technically, any character except `\0` and `/` is valid for a filename,
    /// a range of characters can cause practical and security issues, so we restrict them here.
    ///
    /// A character is considered valid if:
    /// - It is not an ASCII control code
    /// - It is not one of the reserved filename characters `\0` or `/`
    /// - It is not a character which is expanded in bash within `"` double quotes: `$ ` \ !`
    /// - .. a valid bash quote: `' "`
    /// - .. or otherwise particularly dangerous in inadvertent shell expansions: `< > | ? *`
    /// - Note that we allow all forms of parentheses so filename expansions will still need to
    ///   be quoted as per good practice
    fn is_valid(c: &char) -> bool {
        let reserved = ['\0', '/'];
        let expanded = ['$', '`', '\\', '!'];
        let quotes = ['\'', '"'];
        let dangerous = ['<', '>', '|', '?', '*'];

        !c.is_ascii_control()
            && !reserved.contains(c)
            && !expanded.contains(c)
            && !quotes.contains(c)
            && !dangerous.contains(c)
    }

    fn invalid_at_start(c: &char) -> bool {
        c.is_whitespace() || *c == '-'
    }

    pathsegment
        .chars()
        .filter(is_valid)
        .skip_while(invalid_at_start)
        .collect()
}
