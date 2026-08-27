# redbook

This is a rust project related for working with CDDA CD digital audio as per RedBook (IEC 60908:1999). It consists of a library in `src` and a series of binaries in `src/bin`

## MUST USE - MANDATORY INFORMATION & SKILLS

- ALWAYS USE `cargo stage --strict --json` to check your work. This codebase will not compile with `cargo check/clippy/test` as it requires specific libraries. See the `cargo-stage` skill for more details.
- ALWAYS your `graphify` skill to help understand the codebase. Do not use `grep`, `find` or raw file reads until you have first used graphify.
- ALWAYS USE your `jaq` skill to parse json, toml or yaml. Other tools such as `jq` or `python`are not available

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

You are working in a devcontainer. To identify which tools are available to you use your `devcontainer-environment` skill.

You are in a linux environment, hardware access is only implemented for windows. To compile the binaries you MUST USE your `compilation` skill. Compiled binaries will not execute in this environment. Ask the user if you need them to manually run the binary at any point.

