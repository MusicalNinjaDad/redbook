# Graph Report - redbook  (2026-08-29)

## Corpus Check
- 421 files · ~3,077,590 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 843 nodes · 1098 edges · 57 communities (47 shown, 10 thin omitted)
- Extraction: 99% EXTRACTED · 1% INFERRED · 0% AMBIGUOUS · INFERRED: 12 edges (avg confidence: 0.88)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `93458a42`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- CdDrive
- Disc
- Frame
- hex.rs
- TestAlbum
- release_menu
- Item
- graphify Skill
- Rip
- Rust CI Workflow
- Project TODO
- Rust Documentation Checklist
- Exit
- Exit<T>
- Intra-Doc Linking
- load_hex_file
- main
- Tag
- Test Matrix Configuration
- graphify Pipeline
- jaq Skill
- redbook
- Release Selection Data
- doctests.md
- Documentation Basics
- Content Guidelines
- redbook
- Interoperability
- Naming
- Cargo Stage Verification Skill
- Read-the-Docs Cheat Sheet
- Read the Docs
- Flexibility
- Checklist
- Tips and Best Practices
- Standard Markdown Features
- Documentation
- Build-Safely Skill
- Markdown Support in Rustdoc
- Predictability
- Future proofing
- Compilation Skill
- Dependability
- Type safety
- Macros
- Common Issues and Solutions
- Rustdoc-Specific Markdown Extensions
- rust-api-design/SKILL.md
- Special Considerations for Documentation Comments
- Code-Specific Formatting
- Debuggability
- Necessities
- update-indexes.sh
- devcontainer-environment/SKILL.md

## God Nodes (most connected - your core abstractions)
1. `Disc` - 33 edges
2. `Frame` - 30 edges
3. `TestAlbum` - 25 edges
4. `CdDrive` - 21 edges
5. `Msf` - 14 edges
6. `Rust Documentation Checklist` - 13 edges
7. `Content Guidelines` - 13 edges
8. `graphify Skill` - 13 edges
9. `AudioCd` - 12 edges
10. `Intra-Doc Linking` - 12 edges

## Surprising Connections (you probably didn't know these)
- `rip.exe` --semantically_similar_to--> `Project TODO`  [INFERRED] [semantically similar]
  README.md → todo.md
- `compare_toc()` --calls--> `parse_toc()`  [INFERRED]
  tests/parse_toc.rs → src/hex.rs
- `compare_toc()` --references--> `TestAlbum`  [EXTRACTED]
  tests/parse_toc.rs → src/test_fixtures/albums.rs
- `AudioCd` --implements--> `AudioCdExtMut`  [EXTRACTED]
  src/win.rs → src/lib.rs
- `toc_basic_properties()` --references--> `TestAlbum`  [EXTRACTED]
  src/win.rs → src/test_fixtures/albums.rs

## Import Cycles
- 1-file cycle: `src/lib.rs -> src/lib.rs`
- 2-file cycle: `src/test_fixtures/albums.rs -> src/win.rs -> src/test_fixtures/albums.rs`

## Communities (57 total, 10 thin omitted)

### Community 0 - "CdDrive"
Cohesion: 0.06
Nodes (38): Arc, Debug, Drop, Eq, HANDLE, Path, PCWSTR, Send (+30 more)

### Community 1 - "Disc"
Cohesion: 0.07
Nodes (35): B, ExactSizeIterator, PictureType, S, Disc, DiscError, identify_disc_index(), io::Error (+27 more)

### Community 2 - "Frame"
Cohesion: 0.09
Nodes (24): Add, MemSink, N, Output, Rem, AudioCdExtMut, Duration, Frame (+16 more)

### Community 3 - "hex.rs"
Cohesion: 0.08
Nodes (19): ParseIntError, hex_dump(), hex_dump_roundtrip(), hex_to_bytes(), hex_to_bytes_bad_char(), hex_to_bytes_bad_length(), HexErrorKind, parse_toc() (+11 more)

### Community 4 - "TestAlbum"
Cohesion: 0.11
Nodes (12): Discid, Display, Formatter, Option, PathBuf, Result, String, Toc (+4 more)

### Community 5 - "release_menu"
Cohesion: 0.10
Nodes (20): ClapError, Exit, Exit<T>, main(), Error, Exit, From, Infallible (+12 more)

### Community 6 - "Item"
Cohesion: 0.17
Nodes (14): ArtistCreditsExt, Option<T>, Release, ReleaseExt, ReleaseScript, ReleaseStatus, Item, Iterator (+6 more)

### Community 7 - "graphify Skill"
Cohesion: 0.14
Nodes (21): Add and Watch Reference, Exports Reference, Extraction Spec Reference, GitHub and Merge Reference, Hooks Reference, Query Reference, Transcribe Reference, Update Reference (+13 more)

### Community 8 - "Rip"
Cohesion: 0.15
Nodes (13): LogFormat, LogLevel, Rip, Option, PathBuf, String, Exit<T>, LevelFilter (+5 more)

### Community 9 - "Rust CI Workflow"
Cohesion: 0.31
Nodes (10): Dependabot Configuration, Dependabot Automation Workflow, Rust CI Workflow, Binaries Publishing Job, Format Check Job, Lint Job, Publish to crates.io Workflow, Quality Check Job (+2 more)

### Community 10 - "Project TODO"
Cohesion: 0.22
Nodes (9): Redbook Library, rip.exe, tag.exe, toc.exe, Project TODO, AI Image Recognition Task, GUI with Slint Task, Linux Module Task (+1 more)

### Community 11 - "Rust Documentation Checklist"
Cohesion: 0.05
Nodes (42): 1. Summary Line (Required), 2. Detailed Explanation (As Needed), 3. Code Example (Required), 4. Advanced Sections (As Needed), Advanced Patterns, Attributes, Automated Checks, Basic Linking (+34 more)

### Community 12 - "Exit"
Cohesion: 0.33
Nodes (5): Exit, main(), Exit, String, T

### Community 13 - "Exit<T>"
Cohesion: 0.47
Nodes (5): Exit<T>, Error, From, Infallible, Self

### Community 14 - "Intra-Doc Linking"
Cohesion: 0.05
Nodes (40): Advanced Examples, Automatic Disambiguation, Basic Syntax, Best Practices, Common Patterns, Complex Generic Links, Disambiguation in Practice, Disambiguation Prefixes (+32 more)

### Community 15 - "load_hex_file"
Cohesion: 0.67
Nodes (3): load_hex_file(), PathBuf, Vec

### Community 25 - "doctests.md"
Cohesion: 0.05
Nodes (38): Attribute Syntax, Available Attributes, Basic Usage, Code Block Attributes, Documentation Tests (Doctests), Escaping `#` Characters, Example Detection, Example: Multi-step Documentation (+30 more)

### Community 26 - "Documentation Basics"
Cohesion: 0.05
Nodes (32): Common Documentation Sections, Crate-Level Documentation, Documentation Basics, Documentation for Different Item Types, Documenting Components, Enums, Example: Function Documentation, Getting Started with Crate Documentation (+24 more)

### Community 27 - "Content Guidelines"
Cohesion: 0.05
Nodes (37): Accessibility, Additional Lints, Advanced Users, API Guidelines Reference, Beginners, Content Guidelines, Crate-Level Organization, Custom CSS (+29 more)

### Community 28 - "redbook"
Cohesion: 0.06
Nodes (33): Abstraction levels & function length, Available tools, Avoid, Avoid nesting, Best, Better, Binaries overview, Checklist (+25 more)

### Community 29 - "Interoperability"
Cohesion: 0.14
Nodes (14): Binary number types provide `Hex`, `Octal`, `Binary` formatting (C-NUM-FMT), Collections implement `FromIterator` and `Extend` (C-COLLECT), Conversions use the standard traits `From`, `AsRef`, `AsMut` (C-CONV-TRAITS), Data structures implement Serde's `Serialize`, `Deserialize` (C-SERDE), Error types are meaningful and well-behaved (C-GOOD-ERR), Examples, Examples from the standard library, Examples from the standard library (+6 more)

### Community 30 - "Naming"
Cohesion: 0.14
Nodes (13): Ad-hoc conversions follow `as_`, `to_`, `into_` conventions (C-CONV), Casing conforms to RFC 430 (C-CASE), Examples from the standard library, Examples from the standard library, Examples from the standard library, Examples from the standard library, Feature names are free of placeholder words (C-FEATURE), Getter names follow Rust convention (C-GETTER) (+5 more)

### Community 31 - "Cargo Stage Verification Skill"
Cohesion: 0.15
Nodes (12): Cargo Stage Verification Skill, clippy and clippy the tests tasks, Environment, fmt task, How to Check Results, Output Format, Reference Files, Task Breakdown (+4 more)

### Community 32 - "Read-the-Docs Cheat Sheet"
Cohesion: 0.15
Nodes (12): Common Crate Paths, Full approach (all public API):, I need to check if a type has a specific method, I need to check the documentation string for an item, I need to check what methods/fields are available on a type, I need to find all public items in a crate, I need to find all trait implementations for a type, I need to find all types in a module (+4 more)

### Community 33 - "Read the Docs"
Cohesion: 0.15
Nodes (12): 1. Crate Documentation (JSON Format), 1. Name-to-File Index: `./docs/index/name_to_file.json`, 2. Standard Library Documentation (HTML Format), Best Practices, Copy-Paste Queries, Documentation Sources, Indexes, JSON Structure Reference (+4 more)

### Community 34 - "Flexibility"
Cohesion: 0.15
Nodes (12): Advantages of generics, Advantages of trait objects, Caller decides where to copy and place data (C-CALLER-CONTROL), Disadvantages of generics, Disadvantages of trait objects, Examples from the standard library, Examples from the standard library, Examples from the standard library (+4 more)

### Community 35 - "Checklist"
Cohesion: 0.17
Nodes (12): Checklist, Debuggability, Dependability, Documentation, Flexibility, Future proofing, Interoperability, Macros (+4 more)

### Community 36 - "Tips and Best Practices"
Cohesion: 0.18
Nodes (11): 10. Be Consistent, 1. Use Headers Wisely, 2. Keep Paragraphs Short, 3. Use Code Blocks Effectively, 4. Use Tables for Structured Data, 5. Use Lists for Enumerations, 6. Use Links Generously, 7. Use Formatting for Emphasis (+3 more)

### Community 37 - "Standard Markdown Features"
Cohesion: 0.18
Nodes (11): Blockquotes, Code Blocks, Headers, Horizontal Rules, HTML, Images, Links, Lists (+3 more)

### Community 38 - "Documentation"
Cohesion: 0.18
Nodes (10): All items have a rustdoc example (C-EXAMPLE), Cargo.toml includes all common metadata (C-METADATA), Crate level docs are thorough and include examples (C-CRATE-DOC), Documentation, Examples, Examples use `?`, not `try!`, not `unwrap` (C-QUESTION-MARK), Function docs include error, panic, and safety considerations (C-FAILURE), Prose contains hyperlinks to relevant things (C-LINK) (+2 more)

### Community 39 - "Build-Safely Skill"
Cohesion: 0.20
Nodes (9): 1. Check if the feature exists in `UnstableFeature` enum, 2. If the feature is in the enum, 3. If the feature is NOT in the enum, Build-Safely Skill, Creating Feature Requests, Documentation, Quick Reference, Verification Checklist (+1 more)

### Community 40 - "Markdown Support in Rustdoc"
Cohesion: 0.20
Nodes (9): Comprehensive Documentation Example, Custom CSS Classes, Examples in Action, Markdown Support in Rustdoc, Module Documentation Example, Overview, Rustdoc-Specific HTML Features, Summary (+1 more)

### Community 41 - "Predictability"
Cohesion: 0.20
Nodes (10): Constructors are static, inherent methods (C-CTOR), Conversions live on the most specific type involved (C-CONV-SPECIFIC), Examples from the standard library, Examples from the standard library, Functions do not take out-parameters (C-NO-OUT), Functions with a clear receiver are methods (C-METHOD), Only smart pointers implement `Deref` and `DerefMut` (C-DEREF), Operator overloads are unsurprising (C-OVERLOAD) (+2 more)

### Community 42 - "Future proofing"
Cohesion: 0.22
Nodes (8): Data structures do not duplicate derived trait bounds (C-STRUCT-BOUNDS), Examples, Examples from the standard library, Exceptions, Future proofing, Newtypes encapsulate implementation details (C-NEWTYPE-HIDE), Sealed traits protect against downstream implementations (C-SEALED), Structs have private fields (C-STRUCT-PRIVATE)

### Community 43 - "Compilation Skill"
Cohesion: 0.25
Nodes (7): Compilation Commands, Compilation Skill, Important Rule, Linux Targets, Target Discovery, When to Load, Windows Targets

### Community 44 - "Dependability"
Cohesion: 0.25
Nodes (8): Dependability, Destructors never fail (C-DTOR-FAIL), Destructors that may block have alternatives (C-DTOR-BLOCK), Dynamic enforcement, Dynamic enforcement with `debug_assert!`, Dynamic enforcement with opt-out, Functions validate their arguments (C-VALIDATE), Static enforcement

### Community 45 - "Type safety"
Cohesion: 0.25
Nodes (8): Arguments convey meaning through types, not `bool` or `Option` (C-CUSTOM-TYPE), Builders enable construction of complex values (C-BUILDER), Consuming builders, Newtypes provide static distinctions (C-NEWTYPE), Non-consuming builders (preferred), The benefit, Type safety, Types for a set of flags are `bitflags`, not enums (C-BITFLAG)

### Community 46 - "Macros"
Cohesion: 0.29
Nodes (6): Input syntax is evocative of the output (C-EVOCATIVE), Item macros compose well with attributes (C-MACRO-ATTR), Item macros support visibility specifiers (C-MACRO-VIS), Item macros work anywhere that items are allowed (C-ANYWHERE), Macros, Type fragments are flexible (C-MACRO-TY)

### Community 47 - "Common Issues and Solutions"
Cohesion: 0.33
Nodes (6): Common Issues and Solutions, Problem: Code block not syntax highlighted, Problem: Lists not rendering correctly, Problem: Markdown not working in HTML tags, Problem: Smart punctuation causing issues, Problem: Tables not rendering correctly

### Community 48 - "Rustdoc-Specific Markdown Extensions"
Cohesion: 0.33
Nodes (6): Footnotes, Rustdoc-Specific Markdown Extensions, Smart Punctuation, Strikethrough, Tables, Task Lists

### Community 50 - "Special Considerations for Documentation Comments"
Cohesion: 0.40
Nodes (5): Blank Lines, Comment Syntax, Indentation, Nesting, Special Considerations for Documentation Comments

### Community 51 - "Code-Specific Formatting"
Cohesion: 0.50
Nodes (4): Code Highlighting, Code-Specific Formatting, Escaping Backticks in Code, Preserving Whitespace

### Community 52 - "Debuggability"
Cohesion: 0.50
Nodes (3): All public types implement `Debug` (C-DEBUG), `Debug` representation is never empty (C-DEBUG-NONEMPTY), Debuggability

### Community 53 - "Necessities"
Cohesion: 0.50
Nodes (3): Crate and its dependencies have a permissive license (C-PERMISSIVE), Necessities, Public dependencies of a stable crate are stable (C-STABLE)

## Knowledge Gaps
- **347 isolated node(s):** `redbook`, `update-indexes.sh script`, `ReleaseMenu<'d>`, `Rip`, `SelectedTrack` (+342 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **10 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `Disc` connect `Disc` to `CdDrive`, `Frame`, `release_menu`?**
  _High betweenness centrality (0.044) - this node is a cross-community bridge._
- **Why does `TestAlbum` connect `TestAlbum` to `CdDrive`, `Disc`, `hex.rs`?**
  _High betweenness centrality (0.037) - this node is a cross-community bridge._
- **Why does `Markdown Support in Rustdoc` connect `Markdown Support in Rustdoc` to `Tips and Best Practices`, `Standard Markdown Features`, `Common Issues and Solutions`, `Rustdoc-Specific Markdown Extensions`, `Special Considerations for Documentation Comments`, `Code-Specific Formatting`?**
  _High betweenness centrality (0.031) - this node is a cross-community bridge._
- **What connects `redbook`, `update-indexes.sh script`, `ReleaseMenu<'d>` to the rest of the system?**
  _347 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `CdDrive` be split into smaller, more focused modules?**
  _Cohesion score 0.056051587301587304 - nodes in this community are weakly interconnected._
- **Should `Disc` be split into smaller, more focused modules?**
  _Cohesion score 0.06557377049180328 - nodes in this community are weakly interconnected._
- **Should `Frame` be split into smaller, more focused modules?**
  _Cohesion score 0.09131205673758866 - nodes in this community are weakly interconnected._