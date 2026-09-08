pub const CDDA: TRACK_MODE_TYPE = 2;
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct CDROM_READ_TOC_EX {
    pub _bitfield: u8,
    pub SessionTrack: u8,
    pub Reserved2: u8,
    pub Reserved3: u8,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CDROM_TOC {
    pub Length: [u8; 2],
    pub FirstTrack: u8,
    pub LastTrack: u8,
    pub TrackData: [TRACK_DATA; 100],
}
impl Default for CDROM_TOC {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
pub const IOCTL_CDROM_RAW_READ: i32 = 147518;
pub const IOCTL_CDROM_READ_TOC_EX: i32 = 147540;
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct RAW_READ_INFO {
    pub DiskOffset: i64,
    pub SectorCount: u32,
    pub TrackMode: TRACK_MODE_TYPE,
}
pub const RawWithC2: TRACK_MODE_TYPE = 4;
pub const RawWithC2AndSubCode: TRACK_MODE_TYPE = 3;
pub const RawWithSubCode: TRACK_MODE_TYPE = 5;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TRACK_DATA {
    pub Reserved: u8,
    pub _bitfield: u8,
    pub TrackNumber: u8,
    pub Reserved1: u8,
    pub Address: [u8; 4],
}
impl Default for TRACK_DATA {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
pub type TRACK_MODE_TYPE = i32;
pub const XAForm2: TRACK_MODE_TYPE = 1;
pub const YellowMode2: TRACK_MODE_TYPE = 0;
