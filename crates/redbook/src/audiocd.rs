#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// A status update which is provided in the `reply` channel of a [thread_safely::Context] during
/// [rip][AudioCdExt::rip] and [read_track][AudioCdExt::read_track]
#[expect(missing_docs, reason = "well named fields")]
pub struct RipProgress {
    pub track_number: usize,
    pub bytes_processed: usize,
    pub total_bytes: usize,
}
