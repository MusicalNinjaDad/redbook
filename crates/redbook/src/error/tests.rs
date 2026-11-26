use crate::{
    TrackNo,
    toc::error::{CdaError, InvalidTrackNo},
};

use super::*;

#[test]
fn underlying_messages() {
    let error = TrackNo::new(100)
        .map_err(TocError::from)
        .map_err(Error::from)
        .unwrap_err();
    assert_eq!(error.to_string(), "TOC error");

    let tocerror = error.underlying::<TocError>().unwrap();
    assert_eq!(tocerror.to_string(), "invalid track number in TOC");

    let invalid_trackno = error.underlying::<InvalidTrackNo>().unwrap();
    assert_eq!(invalid_trackno.to_string(), "invalid track number: 0x0064");

    assert_eq!(
        error.full_message().to_string(),
        "TOC error: invalid track number in TOC: invalid track number: 0x0064"
    );
}

#[test]
fn cda_error_is_a_top_level_variant() {
    let error = Error::from(CdaError::MissingNullByte);
    assert!(matches!(error, Error::CdaError(_)));
    assert_eq!(error.to_string(), "CDA file error");

    let cda_error = error.underlying::<CdaError>().unwrap();
    assert_eq!(cda_error.to_string(), "missing null byte");

    assert_eq!(
        error.full_message().to_string(),
        "CDA file error: missing null byte"
    );
}

#[test]
fn io_error() {
    let error = TrackNo::new(100)
        .map_err(TocError::from)
        .map_err(Error::from)
        .unwrap_err();

    assert!(error.underlying::<io::Error>().is_none());
    assert!(error.raw_os_error().is_none());

    let io_error = error.into_io_error();

    assert_eq!(io_error.kind(), io::ErrorKind::InvalidData);
    assert_eq!(io_error.to_string(), "TOC error");

    let redbook_error = io_error.downcast::<Error>().unwrap();
    assert_eq!(
        redbook_error.full_message().to_string(),
        "TOC error: invalid track number in TOC: invalid track number: 0x0064"
    );
}

#[test]
fn other_boxes_any_c_good_error_with_fidelity() {
    let error = Error::other(std::fmt::Error);
    assert!(matches!(error, Error::Other(_)));
    assert_eq!(error.to_string(), "other error");
    let source = StdError::source(&error).expect("boxed error should be the source");
    assert!(source.downcast_ref::<std::fmt::Error>().is_some());
    assert!(error.underlying::<std::fmt::Error>().is_some());
    // io::Errors up-convert into the same variant but keep their label
    let io_error: Error = io::Error::other("handcrafted").into();
    assert!(matches!(io_error, Error::Other(_)));
    assert_eq!(io_error.to_string(), "other error");
}

#[test]
fn into_io_error_preserves_kind_of_underlying_io_error() {
    // an io::Error anywhere in the source chain wins over the per-type fallback kind
    let raw = io::Error::from_raw_os_error(5);
    let expected_kind = raw.kind();
    let error = DeviceError::Other(raw);
    let io_error = error.into_io_error();
    assert_eq!(io_error.kind(), expected_kind);
    // the typed error is still the payload, so the tree (and the raw OS code) survives
    let payload = io_error
        .get_ref()
        .and_then(|e| e.downcast_ref::<DeviceError>())
        .expect("payload should be the DeviceError");
    assert_eq!(payload.raw_os_error(), Some(5));
}

#[test]
fn error_report() {
    let rberr: Error = io::Error::other("handcrafted").into();
    let rpt = rberr.full_message();
    assert_eq!(rpt.to_string(), "other error: handcrafted")
}
