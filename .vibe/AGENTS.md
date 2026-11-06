## tooling & skills

- use `jaq` for parsing json. Do not use `grep`, `rg`, `python`or `jq`.
- use `read-the-docs` to understand APIs in this crate & dependencies. Do not use `grep` or `rg`.
- use `graphify` for codebase questions related to semantics and connections.
- use `rg` in place of `grep` it is much faster.
- use `cargo-stage` skill. This codebase will not compile with `cargo check/clippy/test` as it requires specific libraries.
- use your `compilation` skill to compile binaries (hardware access is Windows-only; binaries won’t execute in this Linux environment).

## graphify

This project has a knowledge graph at graphify-out/ with god nodes, community structure, and cross-file relationships.

When the user types `/graphify`, use the installed graphify skill or instructions before doing anything else.

Rules:
- For codebase questions, first run `graphify query "<question>"` when graphify-out/graph.json exists. Use `graphify path "<A>" "<B>"` for relationships and `graphify explain "<concept>"` for focused concepts. These return a scoped subgraph, usually much smaller than GRAPH_REPORT.md or raw grep output.
- Dirty graphify-out/ files are expected after hooks or incremental updates; dirty graph files are not a reason to skip graphify. Only skip graphify if the task is about stale or incorrect graph output, or the user explicitly says not to use it.
- If graphify-out/wiki/index.md exists, use it for broad navigation instead of raw source browsing.
- Read graphify-out/GRAPH_REPORT.md only for broad architecture review or when query/path/explain do not surface enough context.
- After modifying code, run `graphify update .` to keep the graph current (AST-only, no API cost).

## SAFETY

- This crate contains unsafe code. NEVER, EVER MODIFY OR CREATE UNSAFE CODE. If a change to unsafe code is needed STOP, INFORM THE USER of what needs to be done, WAIT FOR THE USER to implement that change. If new unsafe code is needed STOP, INFORM THE USER of what needs to be done, WAIT FOR THE USER to implement that change. If you are asked to REVIEW unsafe code NEVER CHANGE THE CODE, you MAY NOT EDIT any files while reviewing unsafe code. YOU ARE NOT AUTHORISED TO WRITE OR CHANGE UNSAFE CODE - ONLY THE USER MAY MAKE CHANGES TO UNSAFE CODE.