//! Safe and sane wrappers around Windows APIs for CD drive access

#[doc(inline)]
pub use handles::DriveHandle;

mod bindings;

#[expect(
    unsafe_code,
    reason = "conversion to/from raw bytes for test asset dump/load"
)]
mod cdrom_toc;

#[forbid(unsafe_code)]
mod convert;

#[forbid(unsafe_code)]
mod drive;

#[forbid(unsafe_code)]
mod error;

#[expect(unsafe_code, reason = "low level device implementation via ffi")]
mod handles;

#[forbid(unsafe_code)]
mod test_fixtures;

#[cfg(test)]
mod miri {
    use rstest::rstest;

    use super::*;
    use crate::{
        Device, Frame,
        test_fixtures::albums::TestAlbum::{self, *},
        toc::ByteSize,
    };

    #[rstest]
    #[case(DefinitelyMaybe)]
    #[case(TheWallDisc1)]
    #[case(TheWallDisc2)]
    #[case(BatOutOfHellRevamped)]
    #[case(TheBoyWhoKnewTooMuch)]
    fn read_chunk(#[case] album: TestAlbum) {
        let device = DriveHandle::open(&album.assets_path()).unwrap();
        let mut buf = [0_u8; const { Frame::strict_new(10).size().cast::<usize>() }];
        device.read_cdda(Frame::strict_new(150), &mut buf).unwrap();
        let (f, x) = buf.as_chunks::<{ ByteSize::ONE_FRAME.cast::<usize>() }>();
        assert_eq!(x.len(), 0);
        assert_eq!(f.len(), 10);
        let frames: Vec<Frame> = f
            .iter()
            .map(|frameno| {
                let (frameno, zeros) = frameno.split_at(4);
                assert_eq!(zeros, [0_u8; 2352 - 4]);
                u32::from_le_bytes(frameno.try_into().unwrap())
            })
            .map(Frame::strict_new)
            .collect();
        assert_eq!(frames.first(), Some(&Frame::strict_new(150)));
        assert_eq!(frames.last(), Some(&Frame::strict_new(159)));
    }

    #[rstest]
    #[case(DefinitelyMaybe)]
    #[case(TheWallDisc1)]
    #[case(TheWallDisc2)]
    #[case(BatOutOfHellRevamped)]
    #[case(TheBoyWhoKnewTooMuch)]
    fn max_chunk(#[case] album: TestAlbum) {
        let device = DriveHandle::open(&album.assets_path()).unwrap();
        let raw = device.raw_max_chunk().unwrap();
        assert_eq!(raw, album.max_chunk());
    }
}
