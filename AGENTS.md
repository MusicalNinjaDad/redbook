# redbook

This is a rust project related for working with CDDA CD digital audio as per RedBook (IEC 60908:1999). It consists of a library in `src` and a series of binaries in `src/bin`

## MUST USE - MANDATORY INFORMATION & SKILLS

- This crate contains unsafe code. NEVER, EVER MODIFY OR CREATE UNSAFE CODE. If a change to unsafe code is needed STOP, INFORM THE USER of what needs to be done, WAIT FOR THE USER to implement that change. If new unsafe code is needed STOP, INFORM THE USER of what needs to be done, WAIT FOR THE USER to implement that change. If you are asked to REVIEW unsafe code NEVER CHANGE THE CODE, you MAY NOT EDIT any files while reviewing unsafe code. YOU ARE NOT AUTHORISED TO WRITE OR CHANGE UNSAFE CODE - ONLY THE USER MAY MAKE CHANGES TO UNSAFE CODE.

- ALWAYS USE `cargo stage --strict --json` to check your work. This codebase will not compile with `cargo check/clippy/test` as it requires specific libraries. See the `cargo-stage` skill for more details.
- ALWAYS your `graphify` skill to help understand the codebase. Do not use `grep`, `find` or raw file reads until you have first used graphify. `rg` is available as a faster alternative to grep.
- ALWAYS USE your `jaq` skill to parse json, toml or yaml. Other tools such as `jq` or `python`are not available.

## Library overview

Most of what this library provides is "glue", bringing together crates which cover different
parts of the CD audio landscape into a single, coherent whole.

### End-to-end functionality

1. **Hardware access** Read audio data from a CD
2. **Parse & lookup** information on the album *including coverart*, generate tags
   & embeddable coverart
3. **Encode music** to wav or flac

### Structure

There are 3 key entry points to the crate, one each for hardware, CD structure, and music data.

- [AudioCd] & [AudioCdExt]  - for interfacing with hardware
- [Disc]                    - for working with the contents of a CD
- [RippedTrack]             - for the actual music of a given track

## Binaries overview

### rip

Is a basic windows CLI to rip audio CDs to flac. This is the main application.

### toc

Is a technical debug tool to extract the raw windows-API-specific CDROM_TOC from a CD.

### tag

Is a debug tool to list the tags embedded in a flac file.

## Codebase knowledge graph - graphify

This project has a knowledge graph at graphify-out/ with god nodes, community structure, and cross-file relationships.

When the user types `/graphify`, use the installed graphify skill or instructions before doing anything else.

Rules:

- For codebase questions, first run `graphify query "<question>"` when graphify-out/graph.json exists. Use `graphify path "<A>" "<B>"` for relationships and `graphify explain "<concept>"` for focused concepts. These return a scoped subgraph, usually much smaller than GRAPH_REPORT.md or raw grep output.
- Dirty graphify-out/ files are expected after hooks or incremental updates; dirty graph files are not a reason to skip graphify. Only skip graphify if the task is about stale or incorrect graph output, or the user explicitly says not to use it.
- If graphify-out/wiki/index.md exists, use it for broad navigation instead of raw source browsing.
- Read graphify-out/GRAPH_REPORT.md only for broad architecture review or when query/path/explain do not surface enough context.
- After modifying code, run `graphify update .` to keep the graph current (AST-only, no API cost).

## Development Environment

### Dependencies & Stdlib

USE YOUR `read-the-docs` skill to search and read the documentation for this crate, all dependencies and the standard library. Full documentation is available locally. USE IT.

FULL SOURCE for ALL dependencies is available at `/opt/cargo/registry/src/`. You may read the source for dependencies at any time.

FULL SOURCE for the standard library is available at `/opt/rustup/toolchains/nightly-x86_64-unknown-linux-gnu/lib/rustlib/src/rust/library/`. You may read the source for stdlib at any time.

### Available tools

You are working in a devcontainer. To identify which tools are available to you use your `devcontainer-environment` skill.

PROACTIVELY use your skills. They have been created and selected to specifically help with this project.

### Compilation / Building

You are in a linux environment, hardware access is only implemented for windows. To compile the binaries you MUST USE your `compilation` skill. Compiled binaries will not execute in this environment. Ask the user if you need them to manually run the binary at any point.

## Coding standards

### Priority order for where to find standards

ALWAYS use the following rules to understand priorities. If you have conflicting information regarding coding standards, FOLLOW THESE PRIORITIES:

1. THIS DOCUMENT HAS PRIORITY. It contains specific definitions which are relevant to the project and may diverge from generic information.
2. Your `rust-api-design` skill. This reflects the formal rust language guidelines. YOU MUST USE THIS SKILL before beginning to create code. ONLY diverge from this skill where project specific guidance requires it.
3. Use your `cargo-stage` skill: `cargo stage --strict --json` will call clippy. Clippy is set up to lint against as many of the required standards as possible. YOU MUST consider any compiler or lint warnings as errors. Usually you can follow the compilers/clippy's advice to fix an issue but ALWAYS critically review the suggestion before deciding whether it is actually the correct approach. YOU MUST USE THIS SKILL to ensure that you follow all expected coding standards.
4. Only when the above 3 points do not provide guidance should you fall back on generic practices from your knowledge.

### Documentation and comments

#### Documentation

YOU MUST FOLLOW THE RULES IN `rust-api-design` and USE YOUR `documentation` SKILL.

Additionally, for this crate:

- Public `pub` items must be documented with a **target audience: library users**. Documentation should be clear & concise.
- Private, `pub(crate)` and `pub(super)` items must be documented with a **target audience: library maintainers**. You MUST include proper doccomments for these items, as they are provided as invaluable IDE-popups to maintainers. You do not need to include examples for private items.
- You should add the following sections when relevant:
  - `# Notes` section containing a bulleted list of valuable information
  - `# Notes for implementors` section (for traits) with important details for people implementing this trait on a type
  - `# TODOs` section for cases where items have open todos (both public and private items) - this ensures transparency regarding maturity, limitations and makes it easy to find open tasks

#### Comments

- Good code rarely needs comments. The documentation and API design should be sufficient for both "how" and "why" to be obvious.
- Good code rarely needs comments. Functions should read like a natural language paragraph. Well chosen variable names and well chosen statement ordering and abstractions make this possible.
- Sometimes it is important to use a comment to maintain a record of decisions: why a specific architectural choice was made, why a specific statement ordering was used or helpful warning about a caveat / gotcha which forced a specific approach. Such comments should be placed directly above the line that they refer to or appended to the line if short enough.

##### Avoid

```rust
// ignore problems retrieving and parsing data
let _ = disc.update_musicbrainz();
```

##### Better

```rust
let _ignore_failure = disc.update_musicbrainz();
```

##### Best

```rust
#[expect(unused_must_use, reason = "ignore network & parsing errors, data is not critical")]
disc.update_musicbrainz();
```

### Readability

#### Leverage the type system

- Use rust's strengths. Well defined enums & structs with well named fields convey intent clearly & concisely at the call site.

#### Code ordering

- Group impl blocks for traits in the module where they are *most relevant to readers* - this may be:
  - directly below the type definition when the impl is primarily of interest when understanding the type;
  - directly below the trait definition when the impl is primarily of interest when researching the trait or when the type is foreign to the crate;
  - or in exceptional cases, in a third module when the impl is of particular relevance to that module, usually due to cfg gates
- The ordering is in a module designed to make the code easy to navigate: readers working top to bottom shuold have a logical flow, the outline should work as a well-ordered table of contents. Where semantic ordering is not unique use alphabetical sorting within groups
  1. `const`s & `type` aliases
  2. the most "entry-level" / "fundamental" type
  3. `impl Default` if applicable
  4. `impl Type` with functions ordered:
      1. constructors - beginning with `new`
      2. getters, setters, `as_...` where this is effectively a getter
      3. core functionality
      4. conversion functions
  5. `impl Trait` - functional traits
  6. output & conversion traits: custom traits first then `impl Display`, `impl IntoIterator`, `impl From`, `impl FromStr`, etc.
  7. contained data types
  8. the next fundamental type (it is rare to have more than 2 such types in a single module)
- The ordering within a function is designed to make the function read like a natural-language explanation.
  - variable definition occurs at the most relevant point before usage, readers should not need to keep multiple variables in their head while reading the function
  - related spawned threads & async blocks should be defined in logical order, define all related threads/blocks first. And then joined in the same order as defined.

#### Abstraction levels & function length

Function length is driven entirely by readability.

- Orchestrator functions (e.g. `fn main`, `new`) may be long as they should clearly show all orchestration steps. They should **NOT** contain any significant algorithmic logic. They can be longer as each step is simple. They may contain longer, well bounded blocks - the function of such blocks should be immediately identifiable (e.g. by assigning the output to a named variable or matching on a clearly named variable)
- Algorithmic functions should be shorter, focussed on the specific task at hand, and do ONLY ONE THING.
- It is equally confusing to need to make 5 jumps to read the full implementation of a single task (do not emulate Java, C++, Uncle-Bob idioms) as it is to search through a long function that does many things to find the relevant section (no god-functions).
- Adding an abstraction should always make the code simpler to read and reason about.
- An example of a good orchestrator function can be found at `bin/rip/main.rs::main`
  - `Rip::try_parse()?` & `init_tracing()?` are separate functions
  - `match (cd.disc().release(), ripper.non_interactive)` & `let selected_track = match (ripper.all, ripper.track_number)` are larger orchestration blocks representing the core raison-d'être for the function and their purpose is clearly identifiable from the opening line, making it easy to skip or dive into the related block
- An example of well-separated algorithmic functions can be found at `disc.rs::impl Disc`
  - `update_musicbrainz` only handles the logic of fetching data from the network leveraging
  - `set_musicbrainz` to store the data and maintain invariants (which can then also be a useful pub fn)
  - `Toc::musicbrainz_id` to generate the id (which can then also be a useful pub fn)
  - `Discid::fetch()`& `mb_stuff.execute()` would belong directly inside `update_musicbrainz` if they were not provided by a 3rd-party crate

#### Avoid nesting

Aim to keep code as flat as possible. Obey the zen of python "flat is better than nested" but remember this is not python, go or any other language, this is rust.

As a rule of thumb, nested code should be longer than it is wide: more lines inside each level of nesting than the nest-depth of that level.

Use the following tips to help avoid nesting:

- use `?` to propogate residuals. `impl From<SomeError> for <OtherError>` where needed.
- use `.unwrap_or_default()`, `map_err()`, `.or_else()` to avoid banal `match`es on `Try`-types. Leverage `crate try_v2::Extract` to provide these methods on custom Try-types.
- NEVER USE `if ... else if ... else` CHAINS. ALWAYS USE `match`, this removes a whole class of bugs by enabling the compiler to validate that all cases are considered.
- Leverage a functional style with `.map()`, `.and_then()` chaining. Use the newly stable `ok()`, `then()`, ... functions to avoid `if bool`. Use the unstable `bool.toggle()` to improve readability: `flag.toggle().then_some(1)` is better (more explicit) than `!flag.then_some(1)`
- Use match guards, including `if let` guards to avoid nesting `match ... { if { ... } }`
  - prefer `match ... { pattern if ... => }` but `match (someenum, somebool) { (pattern, true) => ... }` is often best
  - NEVER use `if .is_some()` in a match guard - prefer `match (someenum, someoption) { (pattern, Some(_)) => ... }`
  - use `&&` chaining to avoid nested `if` guards

### Experimental features

This codebase is designed to use a nightly toolchain. This is formally documented in `rust-toolchain.toml`. Use experimental features where they provide significant improvements to the readability and/or maintainability of the code or where they enable a more ergonomic API design.

ALL unstable features MUST be gated using `build-safely` via `#![cfg_attr(unstable_FEATURENAME, feature(FEATURENAME)]`. YOU MUST USE your `build-safely` skill when enabling a new unstable feature.

## Checklist

- [ ] Used MANDATORY SKILLS for baseline coding standards: `rust-api-design` & `documentation`
- [ ] Followed project specific coding standards:
  - Documentation (targets correct audience, includes additional sections as needed) & Comments (only used to warn future maintainers about a specific design choice)
  - Readability (leverage type system, code ordering, function length, nesting)
  - Experimental features use MANDATORY SKILL `build-safely`
- [ ] Worked in an iterative manner, leveraging `cargo-stage` to identify next steps
- [ ] Used MANDATORY SKILL `cargo-stage` for all verification
- [ ] All errors from `cargo-stage` are resolved
- [ ] Meaningful reasons are given for all uses of `#[expect()]`
- [ ] NO CHANGES MADE TO UNSAFE CODE
