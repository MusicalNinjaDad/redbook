# Graph Report - /workspaces/redbook  (2026-08-24)

## Corpus Check
- 42 files · ~97,870 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 377 nodes · 639 edges · 25 communities (18 shown, 7 thin omitted)
- Extraction: 98% EXTRACTED · 2% INFERRED · 0% AMBIGUOUS · INFERRED: 12 edges (avg confidence: 0.88)
- Token cost: 0 input · 0 output

## Community Hubs (Navigation)
- Windows CD Audio
- Disc Module
- General Utilities
- Hex Dump
- Test Fixtures
- rip CLI Main
- MusicBrainz
- graphify Skill References
- rip CLI
- GitHub Workflows
- Project Documentation
- Core Library Types
- tag CLI Main
- tag Error Handling
- Tagging
- Test Fixtures Mod
- Build
- tag CLI
- CI Matrix Config
- graphify Pipeline
- jaq Skill
- Package
- Test Data

## God Nodes (most connected - your core abstractions)
1. `Disc` - 33 edges
2. `Frame` - 29 edges
3. `TestAlbum` - 25 edges
4. `CdDrive` - 21 edges
5. `Msf` - 14 edges
6. `graphify Skill` - 14 edges
7. `AudioCd` - 12 edges
8. `ParseHexError` - 10 edges
9. `new()` - 9 edges
10. `CDROM_TOC` - 9 edges

## Surprising Connections (you probably didn't know these)
- `rip.exe` --semantically_similar_to--> `Project TODO`  [INFERRED] [semantically similar]
  README.md → todo.md
- `compare_toc()` --calls--> `parse_toc()`  [INFERRED]
  tests/parse_toc.rs → src/hex.rs
- `compare_toc()` --references--> `TestAlbum`  [EXTRACTED]
  tests/parse_toc.rs → src/test_fixtures/albums.rs
- `toc_basic_properties()` --references--> `TestAlbum`  [EXTRACTED]
  src/win.rs → src/test_fixtures/albums.rs
- `main()` --calls--> `release_menu()`  [INFERRED]
  src/bin/rip/main.rs → src/bin/rip/_releases.rs

## Import Cycles
- 1-file cycle: `src/lib.rs -> src/lib.rs`
- 2-file cycle: `src/test_fixtures/albums.rs -> src/win.rs -> src/test_fixtures/albums.rs`

## Communities (25 total, 7 thin omitted)

### Community 0 - "Windows CD Audio"
Cohesion: 0.05
Nodes (39): Arc, Debug, Drop, Eq, HANDLE, Path, PCWSTR, Send (+31 more)

### Community 1 - "Disc Module"
Cohesion: 0.07
Nodes (31): ExactSizeIterator, Disc, DiscError, identify_disc_index(), new(), Discid, Display, Error (+23 more)

### Community 2 - "General Utilities"
Cohesion: 0.13
Nodes (17): Add, MemSink, N, Output, Rem, Duration, Frame, leadin_compensation() (+9 more)

### Community 3 - "Hex Dump"
Cohesion: 0.08
Nodes (19): ParseIntError, hex_dump(), hex_dump_roundtrip(), hex_to_bytes(), hex_to_bytes_bad_char(), hex_to_bytes_bad_length(), HexErrorKind, parse_toc() (+11 more)

### Community 4 - "Test Fixtures"
Cohesion: 0.11
Nodes (12): Discid, Display, Formatter, Option, PathBuf, Result, String, Toc (+4 more)

### Community 5 - "rip CLI Main"
Cohesion: 0.10
Nodes (20): ClapError, Exit, Exit<T>, main(), Error, Exit, From, Infallible (+12 more)

### Community 6 - "MusicBrainz"
Cohesion: 0.17
Nodes (14): ArtistCreditsExt, Option<T>, Release, ReleaseExt, ReleaseScript, ReleaseStatus, Item, Iterator (+6 more)

### Community 7 - "graphify Skill References"
Cohesion: 0.14
Nodes (22): Agent Instructions, Add and Watch Reference, Exports Reference, Extraction Spec Reference, GitHub and Merge Reference, Hooks Reference, Query Reference, Transcribe Reference (+14 more)

### Community 8 - "rip CLI"
Cohesion: 0.15
Nodes (13): LogFormat, LogLevel, Rip, Option, PathBuf, String, Exit<T>, LevelFilter (+5 more)

### Community 9 - "GitHub Workflows"
Cohesion: 0.31
Nodes (10): Dependabot Configuration, Dependabot Automation Workflow, Rust CI Workflow, Binaries Publishing Job, Format Check Job, Lint Job, Publish to crates.io Workflow, Quality Check Job (+2 more)

### Community 10 - "Project Documentation"
Cohesion: 0.22
Nodes (9): Redbook Library, rip.exe, tag.exe, toc.exe, Project TODO, AI Image Recognition Task, GUI with Slint Task, Linux Module Task (+1 more)

### Community 11 - "Core Library Types"
Cohesion: 0.36
Nodes (5): Option, String, TocEntry, Track, Track<'meta>

### Community 12 - "tag CLI Main"
Cohesion: 0.33
Nodes (5): Exit, main(), Exit, String, T

### Community 13 - "tag Error Handling"
Cohesion: 0.47
Nodes (5): Exit<T>, Error, From, Infallible, Self

### Community 14 - "Tagging"
Cohesion: 0.40
Nodes (4): B, PictureType, S, Self

### Community 15 - "Test Fixtures Mod"
Cohesion: 0.67
Nodes (3): load_hex_file(), PathBuf, Vec

## Knowledge Gaps
- **19 isolated node(s):** `redbook`, `ReleaseMenu<'d>`, `Rip`, `SelectedTrack`, `TocEntry` (+14 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **7 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `Disc` connect `Disc Module` to `Windows CD Audio`, `General Utilities`, `rip CLI Main`?**
  _High betweenness centrality (0.216) - this node is a cross-community bridge._
- **Why does `TestAlbum` connect `Test Fixtures` to `Windows CD Audio`, `Disc Module`, `Hex Dump`?**
  _High betweenness centrality (0.182) - this node is a cross-community bridge._
- **Why does `Frame` connect `General Utilities` to `Windows CD Audio`, `Disc Module`, `Core Library Types`, `Test Fixtures`?**
  _High betweenness centrality (0.143) - this node is a cross-community bridge._
- **What connects `redbook`, `ReleaseMenu<'d>`, `Rip` to the rest of the system?**
  _19 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Windows CD Audio` be split into smaller, more focused modules?**
  _Cohesion score 0.05480769230769231 - nodes in this community are weakly interconnected._
- **Should `Disc Module` be split into smaller, more focused modules?**
  _Cohesion score 0.07467532467532467 - nodes in this community are weakly interconnected._
- **Should `General Utilities` be split into smaller, more focused modules?**
  _Cohesion score 0.13174603174603175 - nodes in this community are weakly interconnected._