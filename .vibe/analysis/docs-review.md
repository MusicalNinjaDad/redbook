# redbook Documentation Review

Review of all remaining documentation in `crates/redbook/src` (library only; `src/bin/*` not reviewed),
following the polished crate-level docs and public-API cleanup.

**Method**: every module read in full; doc claims cross-checked against the current API and against
the rendered rustdoc JSON (`docs/x86_64-pc-windows-msvc/doc/redbook.json`, current with source).
Intra-doc links were verified programmatically by comparing link references in `docs` against the
`links` map rustdoc records per item — unresolved references are listed in section 2.

**Guide applied** (per review brief):

- Examples only at: modules, "meaty" types (mentioned in the library overview), and significant
  methods/constructors *where they differ from the type-level example*.
- No `# Arguments` sections on functions; no `# Returns` sections on simple getters —
  **exception: `win::sys::mocks`**.
- Non-public item docs targetted at project developers, succinct, IDE-popup friendly.

---

## Priority summary

| # | Category | Count | Severity |
|---|----------|-------|----------|
| 1 | Incorrect / stale information | 14 | High — misleads users |
| 2 | Broken / stale intra-doc links | 7 verified | High — emits `broken_intra_doc_links` warnings |
| 3 | Placeholder / missing docs (`TODO New docs`, `TODO: Docs`, `expect(missing_docs)`) | ~25 items across 4 modules | High — rendered as noise or absent |
| 4 | Bloat to remove (duplicate examples, redundant sections) | ~15 items | Medium — refactoring burden |
| 5 | Non-public doc quality (stale comments, dev-facing wording) | ~10 items | Low–Medium |

---

## 1. Incorrect or stale information

### User-facing (public API)

| Location | Problem | Suggested fix |
|----------|---------|---------------|
| `disc.rs:93` `Disc::new` | Summary says "from a table of contents, tracks, **and leadout frame**" — the leadout parameter no longer exists. The `# Notes` also claims validation of "the leadout frame must match the TOC's leadout value", which the implementation does not check (it validates track number, start and duration only). | Reword to "from a table of contents and its tracks"; correct the validation list. |
| `track.rs:144` `RippedTrack` | "wraps the raw audio data from a CD track **along with its track number**" — the struct holds `tags`, `coverart`, `raw_data`; there is no track number (and `tags`/`coverart` aren't mentioned). Stale from the old API. | Rewrite summary around tags + coverart + raw CDDA data. Field docs "Track details" / "Cover art" are also too weak for the fields that carry them. |
| `toc.rs:188` `TocEntry::is_data_track` | Copy-paste error: doc says "Is an audio track" on the data-track predicate. | "Is a data track (based on control code & track number)". |
| `toc.rs:644` `TocEntries::audio_tracks` | "it will stop each time it encounters a non-audio track or a session leadout" — the implementation *filters* (`is_audio_track().then(..)` + `flatten`) and continues past data tracks; the leadout is consumed as the end-marker for the final audio track, not a stop condition. The `FusedIterator` framing is also a non-sequitur (the point is it yields audio tracks only, and the last window needs a following entry). | Describe actual behaviour: yields audio tracks with durations computed from the next entry; data tracks are skipped, the leadout terminates the final track. Drop the TODO #110 from rendered docs (move to a code comment). |
| `toc.rs:705` `TocEntriesIter` | "`into_iter()` … (provided by the [`Iterator`] trait)" — `into_iter` comes from `IntoIterator`, not `Iterator`. | Fix trait name (the link itself resolves, to the wrong claim). |
| `drive.rs:62` `Drive::open` | "long-running reads via `[rip]` & `[read_track]`" — `Drive` has no `rip` method (it exists on `AudioCd`); stale from the pre-cleanup API. | "via `read_track`" (and `rip` on `AudioCd` if you want to mention it). |
| `drive.rs:66-67` `Drive::cx` | References `[with_context]` and `[add_context]` — neither method exists any more. | "If no context was provided when opening" (the context now comes in via `Drive::open`). |
| `drive.rs:202` `CdDrive::new` | "Use [`open`][Device::open] instead" — the link resolves to `Device::open`, which does not take a `Context<RipProgress>`; the correct replacement is `Drive::open` (the one this type implements). | Change target to `Drive::open`. |
| `disc.rs:335` `set_release_by_id` | "Providing an invalid **index** will make no change" — takes a release *id* (string), not an index (copy-paste from `set_release`). | "Providing an invalid id…". |
| `disc.rs:470` `update_musicbrainz` | "…must be set manually using `set_release()` and `reset_disc_index()`" — there is no manual disc-index setter; `reset_disc_index` is an automatic recompute. Also `reset_disc_index`'s own doc (`disc.rs:422` area) says multi-disc releases get `None`, but the implementation auto-matches by track offsets and only returns `None` when ambiguous. | Reword: release must be selected with `set_release`/`set_release_by_id`; disc index is re-derived automatically and is `None` only when it cannot be uniquely identified. |
| `tagging.rs:173` `VorbisTagExt::directory` | Doc says "Album - Artists/Album title **[Disc n]**" — the implementation produces `Artist/Title` only; no disc number. | Remove `[Disc n]` from the documented format (or implement it — a code decision, not a docs one). |
| `win/convert.rs:147` (area) `WinString::as_pcwstr` | "see `win::drive` for examples" — `win::drive` contains no examples. | Point at `win::sys::handles` (the actual callsites) or drop the sentence. |
| `audiocd.rs:25` (area) `AudioCd` | "Direct construction… on linux it is the `/dev/...` entry" — there is no linux implementation (crate is currently windows-only; mocks only make it *compile*). | Qualify as planned/other-platform behaviour or drop. |

### Non-public / internal

| Location | Problem |
|----------|---------|
| `win/sys/handles.rs:48-50` `DriveHandle` | TODO says "Consider how `DuplicateHandle` could help provide Clone" — `Clone` **is** implemented via `DuplicateHandle` directly below (handles.rs:90). Stale TODO. |
| `win/sys/handles.rs:77` (area) | Code comment "FIX SAFETY requirements of `as_handle`, `as_handle_mut`, `CreateFile2`" — `as_handle`/`as_handle_mut` no longer exist; only the `CreateFile2` part is still relevant. |
| `AGENTS.md` (repo root, "Library overview") | Lists `[AudioCdExt]` as a key entry point — the trait was removed in the API cleanup (a leftover `AudioCdExt` tracing-span name survives at `audiocd.rs:79`). Update AGENTS.md and the span name. |
| `toc.rs:221` `Frame` TODO | "impl Display" — `Display` is implemented (`toc.rs:226`). Remove; keep the repr-transparent item as a code TODO if still wanted. |
| `toc.rs:459` `Msf` TODO | "impl Display" — still true (no `Display` impl exists); fine to keep, but it renders in public docs — prefer a code comment. |

---

## 2. Broken intra-doc links (verified against rustdoc JSON `links` maps)

These references appear in `docs` but have **no entry** in the item's resolved `links` — i.e. rustdoc
would emit `broken_intra_doc_links` warnings.

| Location | Broken reference | Reason |
|----------|------------------|--------|
| `drive.rs:46` `Device::raw_toc` | `[read_toc]` | Unqualified method name doesn't resolve; use `[read_toc][Device::read_toc]` or `` `[Device::read_toc]` ``. |
| `drive.rs:62` `Drive::open` | `[rip]`, `[read_track]` | `rip` doesn't exist on `Drive`; `read_track` exists but unqualified doesn't resolve. |
| `drive.rs:66-67` `Drive::cx` | `[with_context]`, `[add_context]` | Methods removed in API cleanup. |
| `win/sys/cdrom_toc.rs:43` `CDROM_TOC::from_raw_bytes` | `[CdDrive::toc_as_raw_bytes][super::drive::CdDrive::toc_as_raw_bytes]` | Method no longer exists — the raw dump is now `Device::raw_toc`. Doc sentence should be reworded around `raw_toc`. |
| `win/sys/mocks.rs:103` (area) `CreateFile2` | `[`last_os_error`][io::Error::last_os_error]` | Unresolved (likely needs the full `std::io::Error::last_os_error` path). |
| `win/sys/mocks.rs:255` (area) `SetupDiEnumDeviceInterfaces` | `[`last_os_error`][std::io::Error::last_os_error]` | Unresolved despite full path — check with a `cargo doc` warning run. |
| `win/sys/handles.rs:43` (area) `DriveHandle` | `[into_drive_handle][HANDLE::into_drive_handle]` | `IntoDriveHandle` is a private, sealed trait — the path is not nameable in docs. Describe in prose instead. |

Checked and **fine** (common false positives, no action): `[wiki]` on `Frame::MAX`, the
`[pathformats]`/`[traditional]`/`[dosdevicepath]`/`[normalisation]`/`[longpath]` refs on `WinPath`,
and the `[docs_*]` refs on the mock FFI functions are URL reference definitions, not intra-doc links;
`[open][Self::open]` on `DriveHandle` resolves to its `Device::open` impl; `[Self::drop]` in the
`Clone` docs resolves.

---

## 3. Placeholder / missing documentation

### `#![expect(missing_docs)]` suppressions — should be eliminated or explicitly re-scoped

| Module | Suppression | Undocumented public items |
|--------|-------------|---------------------------|
| `audiocd.rs:1` | `#![expect(missing_docs, reason = "WIP")]` | **`AudioCd::open`, `rip`, `disc`, `disc_mut`, `path`, `toc`, `toc_entries` — the main entry point's entire method surface.** Given `AudioCd` is the crate's headline type these are the highest-value missing docs. `toc()` vs `toc_entries()` in particular need a sentence each explaining why there are two (`cdtoc::Toc` from the parsed disc vs `TocEntries` cached from the drive). |
| `tagging.rs:1` | `#![expect(missing_docs, reason = "needs update")]` | `ArtistCreditsExt` (trait + `artist_names`/`artist_ids`), `PictureExt` (trait + `from_jpeg` + `filename`), `ExtendVorbisTag` (trait + `extend_vorbis`). `PictureExt::filename` and `VorbisTagExt::add_to` are used in the crate-level example, so these render as bare names next to documented example code. |

### `/// TODO: Docs` placeholders (rendered verbatim in rustdoc)

- `drive.rs:192` — `CdDrive<DEV>` (struct)
- `drive.rs:255` — `CdDrives<DEV>` (struct)
- `win/drive.rs:25` — `DeviceInfoSetIter` (public via `Device::Iter`)

Each needs a real summary line; `CdDrive`/`CdDrives` are named in the library overview
("general versions of `CdDrives` & `CdDrive`"), so they qualify for short examples too
(e.g. `CdDrives::<DriveHandle>::all(cx)` — the type-parameterised spelling is the interesting bit).

### `# Examples` headings containing only ` TODO New docs` (track.rs)

Rendered as an empty "Examples" section followed by literal "TODO New docs" text:

- `track.rs:29` (`Track`), `:72` (`track_number`), `:86` (`title`), `:107` (`filename`), `:136` (`meta`),
  `:154` (`RippedTrack`), `:180` (`to_flac`), `:223` (`to_wav`)

Per the reduced-example policy, most of these headings should simply be **deleted** rather than
filled: `Track` and `RippedTrack` are overview types and deserve one type-level example each
(`RippedTrack`'s can be lifted from the crate-level example); the methods (`track_number`, `title`,
`meta`, `to_flac`, `to_wav`) should get no separate examples — they are covered by the type-level and
crate-level examples.

### Rendered `# TODO` sections in public docs

These belong in code comments / issue tracker, not in rendered documentation:

- `toc.rs:34` (`TocEntry`: "TODO - Example")
- `toc.rs:647` (`audio_tracks`: TODO #110)
- `audiocd.rs:97` (`RipProgress`: TODO #114)
- `disc.rs:590` (`update_thumbnails`: TODO #62)
- `win/sys/mod.rs:41` (area) (`cdrom_toc` module: "consider making some of this public" — fine as internal, but it is a `///` on a private module; acceptable)
- `win/convert.rs:139` (area) (`From<&[u16]> for WinString`: "TODO: validate is valid UTF-16")

---

## 4. Bloat to remove

### Duplicate / near-duplicate examples (highest burden on refactoring)

| Location | Issue | Recommendation |
|----------|-------|----------------|
| `disc.rs:35-56` `Disc` (type) vs `disc.rs:104-117` `Disc::new` vs `disc.rs:177-198` `release` vs `disc.rs:703-727` `tag_for` | Four examples with the same skeleton (`Toc::from_cdtoc("1+96+2D2B")`, build `tracks`, `Disc::new`, `update_musicbrainz`). `Disc::new`'s is a strict subset of the type-level example; `release`'s adds only the `set_release_by_id` branch; `tag_for`'s adds only the `tags.title()` call. | Keep the **type-level** example on `Disc` (it is an overview type) and extend it minimally to cover release selection and tag generation. Delete the examples from `Disc::new`, `release` and `tag_for`, leaving a one-line "see the [`Disc`] example" where useful. |
| `toc.rs:242-254` `Frame::new`, `toc.rs:266-275` `Frame::as_usize`, `toc.rs:277-297` `Frame::relative_to_leadin`, `toc.rs:472-484` `Msf::new`, `toc.rs:494-512` `Msf::relative_to_leadin` | Five method-level examples. `Frame::new`/`as_usize` are trivial; both `relative_to_leadin` examples demonstrate the identical "183 → 33" computation. | Keep the `Frame` and `Msf` **type-level** examples (both good); delete all five method examples and the `# Arguments`/`# Returns` sections that accompany them. The one conversion worth keeping as a doctest is `From<Duration> for Msf` (`toc.rs:443-455`) because it documents non-obvious truncation semantics. |
| `disc.rs:173-175` `release` `# See also` | Lists the three methods already linked in the preceding `# Note` paragraph. | Delete the section; rustdoc's trait/method sidebars cover discovery. |

### Redundant sections (functions don't need `# Arguments`; simple getters don't need `# Returns`)

- `toc.rs:242` `Frame::new` — `# Arguments` naming its single parameter.
- `toc.rs:277` `Frame::relative_to_leadin`, `toc.rs:494` `Msf::relative_to_leadin` — `# Returns` restating the signature.
- `track.rs:79` `title`, `:95` `meta`, `:124` `filename`, `:168` `to_flac`, `:210` `to_wav` — `# Returns` sections restating the signature (several also have `# Notes` duplicating the summary sentence; `filename`'s "always 2 digits" appears in summary, notes *and* the docstring body).
- `disc.rs:669` `save_cover_art` — `# Returns` is arguably justified (`Option<io::Result<PathBuf>>` is genuinely odd); if kept, it is the only one that should survive this cull. `disc.rs:703` `tag_for`'s `# Returns` should go.
- `track.rs:16` `Track` `# Type Parameters` — rustdoc renders lifetimes; fold the one useful line ("`'meta` is the lifetime of the MusicBrainz metadata reference") into `# Notes` and delete the section header.
- `win/sys/cdrom_toc.rs:173` `From<&TRACK_DATA> for TocEntry` — `# Arguments` on a single-argument `From` impl, in an internal module. Keep the `# Notes` (endianness/lead-in facts are useful), drop `# Arguments` and the `# TODOs` (move to code comment).

**Exception — `win::sys::mocks`**: the `# Arguments` / `# Returns` / `# SAFETY` sections there are
deliberate and valuable (they document the *real* FFI contract plus mock restrictions, per the
crate-level safety claims). Leave their structure alone; see section 5 for wording fixes only.

---

## 5. Non-public item docs (developer-facing)

Mostly good: `win::sys::handles` safety comments, `Disc` private fields, `sanitise`/`is_valid` in
`tagging.rs`, and `win/mod.rs` constants are well-targeted. Issues:

| Location | Problem |
|----------|---------|
| `win/sys/mocks.rs:103` `CreateFile2` | "`lpfilename` must be a valid pointer to a `&[16]`" — should be `&[u16]`. |
| `win/sys/mocks.rs:316` `DuplicateHandle` | "`hsourcehandle` must be a valid pointer to a `HANDLE` as defined at [`std::ptr::read`]" — garbled: `hsourcehandle` is a handle *value*; the pointer requirement belongs to `lptargethandle` (which the next bullet covers). Also "the handle will duplicated" (missing "be"). |
| `win/sys/mocks.rs:411` `GetCurrentProcess` | "`unsafe` is simply applyed by default" — typo: "applied". |
| `win/sys/mocks.rs:70` / `:83` | `CloseHandle` / `SetupDiDestroyDeviceInfoList` are the only mock FFI functions with no doc at all; the inline TODOs about validating closure in error cases are fine as comments, but a one-liner each ("mock: always succeeds; see TODO re closure tracking") would keep the set consistent. |
| `win/sys/handles.rs:538` `get_member` | "This is an wonderful, 3-step process" — typo "a wonderful"; the explanation itself is good and worth keeping. |
| `win/sys/handles.rs:510` (area) `DeviceInfoSet` | The struct has no doc while all its methods do; one line ("RAII wrapper around an `HDEVINFO` from `SetupDiGetClassDevsW`, destroyed on `Drop`") would complete the module. |
| `hex.rs:15` `ParseHexError::kind` | "Outputs the detailed cause of parsing an integer failing" — garbled and too narrow (returns the `HexErrorKind`, which may be `NotPairs`). `hex.rs:27` "occured" → "occurred". (Module is `#[doc(hidden)]`, so low priority.) |
| `win/cda.rs:210` (area) `tracks` | The "guaranteed sorted" claim silently depends on Windows zero-padding track filenames (`Track03.cda`) — confirmed true for the fixtures; worth stating the assumption explicitly for future maintainers. |
| `drive.rs:55` `Drive` trait | "entities which directy access hardware" — typo "directly". |
| `win/convert.rs:1` | Module doc: "ideosyncracies" → "idiosyncrasies". |
| `disc.rs:588` `update_thumbnails` | "Get all tumbnails" — typo "thumbnails", and "Fetch" is the accurate verb (it's a network call). |

---

## 6. Module-by-module summary

| Module | State | Main actions |
|--------|-------|--------------|
| `lib.rs` | Good (already polished) | — |
| `audiocd.rs` | **Needs work** | Document all `AudioCd` methods (remove `expect(missing_docs)`); move TODO #114 out of rendered docs. |
| `drive.rs` | **Needs work** | Fix stale `[rip]`/`[with_context]`/`[add_context]` references; fix `CdDrive::new`'s link to `Drive::open`; replace the three `TODO: Docs` placeholders. |
| `toc.rs` | Mixed | Fix `is_data_track` copy-paste, `audio_tracks` behaviour claim, `TocEntriesIter` trait claim; delete 5 method-level examples + `# Arguments`/`# Returns`; drop stale `impl Display` TODO. Content on `TocEntry::from_scsi_readtoc_0010b` and `Control` is good and should stay. |
| `disc.rs` | Mixed | Fix `Disc::new` summary/notes (leadout), `set_release_by_id`, `reset_disc_index`/`update_musicbrainz` wording, `update_thumbnails` summary; consolidate 4 examples into the type-level one. |
| `track.rs` | **Needs work** | Fix `RippedTrack` stale summary; replace 8 `TODO New docs` placeholders (delete most headings, write 2 type-level examples); trim `# Returns`/`# Type Parameters`. |
| `tagging.rs` | **Needs work** | Remove `expect(missing_docs)`; document the four extension traits properly (`PictureExt`/`VorbisTagExt` are used in the crate example); rewrite the cryptic `VorbisTagExt` method docs; fix `directory()` `[Disc n]` claim. |
| `hex.rs` | OK (doc-hidden) | Minor: `kind()` wording, "occured" typo. |
| `win/mod.rs` | Good | — |
| `win/cda.rs` | Good | Optional: state the zero-padding assumption in `tracks`. |
| `win/convert.rs` | Good | Typo in module doc; fix `as_pcwstr` "see … for examples"; move UTF-16 TODO to code comment. |
| `win/drive.rs` | Needs one item | `DeviceInfoSetIter` `TODO: Docs`. |
| `win/sys/mod.rs` | Good (internal) | — |
| `win/sys/bindgen.rs` | N/A | Generated; intentionally undocumented. |
| `win/sys/cdrom_toc.rs` | Good (internal) | Fix broken `toc_as_raw_bytes` link; drop `# Arguments` on the `From` impl. |
| `win/sys/handles.rs` | Good | Remove stale `DriveHandle` TODO (Clone exists); fix stale `as_handle` comment; typo "implemenation" (`Clone` docs, line 165) and "publically" (line 46); one-line doc for `DeviceInfoSet`. |
| `win/sys/mocks.rs` | Good (intentionally verbose) | Wording only: `&[16]`→`&[u16]`, `DuplicateHandle` safety bullet, "applyed", two missing one-liners. |
| `test_fixtures` | Good (doc-hidden) | — |

---

## 7. Verified as correct (no action)

- `Frame`/`Msf` arithmetic and conversion docs (incl. the 180_123 ms truncation example, the
  `Msf::new(1,30,30)` → `Duration` example, and the 1/75 s & 2352-byte constants).
- `Control` bit-flag table and `Msf` bounds (0-59 / 0-59 / 0-74, ~59:59.986 max).
- `Device` trait docs (`read_cdda` `ErrorKind::Unsupported` contract matches the implementation).
- `Drive::read_track` default-method doc matches chunked-read behaviour.
- `MAX_CHUNK_FRAMES`/`MAX_CHUNK_BYTES`/`LEADIN`/`FRAME_SIZE` internal constants.
- `WinPath`/`WinString`/`Sector`/`Guid` docs incl. `MAX_PATH_CHARS` = 265 accounting for `\\.\` + NUL.
- `CdaFile` parsing docs (RIFF/CDDA layout, absolute-vs-relative frame note).
- `DriveHandle` `Send` safety rationale, `Clone` semantics (non-equal clones, panic on failure) —
  all match the implementation.
- All mock FFI `# SAFETY`/`# Arguments` sections (the deliberate exception) are consistent with the
  assertions the mocks actually enforce.
- `to_flac` returning `MemSink<u8>` is consistent with `flacenc::bitsink::ByteSink` (a type alias
  for `MemSink<u8>`).
