---
name: brainstorm
description: "Read-only brainstorm/analysis mode: generate ideas, options, and analysis WITHOUT editing any code or files. No edit/write/sed/rm/git mutations — safe to run with smart-approve. Load this when the user wants thinking, not changes."
user-invocable: true
allowed-tools:
  - bash
  - read_file
  - grep
  - skill
  - ask_user_question
  - web_search
---

# Brainstorm Mode

You are in read-only brainstorm mode. The user wants ideas, options, analysis, or exploration — not changes. This mode is safe to run with smart-approve: every tool you may call is read-only.

## Hard rules (never break these)

1. **No file changes anywhere.** Never call `edit` or `write_file`, and never run any command that creates, modifies, or deletes a file: no `sed -i`, `awk -i`, `tee`, `touch`, `rm`, `mv`, `cp`, `mkdir`, `chmod`, no redirects into files (`>`, `>>`), no `git checkout`/`stash`/`commit`/`push`/`apply`. If a change seems needed, describe it in your response instead — do not make it.
2. **Bash only for read-only commands.** Allowed: `rg`, `ls`, `find`, `cat` (prefer `read_file`), `jaq`, and read-only git (`git status`, `git log`, `git diff`, `git show`, `git branch`). If a command would write anything, do not run it. When unsure, don't run it — say what you would have run and why you didn't.
3. **No builds, tests, or compiles** unless the user explicitly asks — they are slow and write build artifacts. Analyze by reading code, not running it. Do not load `cargo-stage` or `compilation` unless asked.
4. **If the user asks for an edit mid-session:** decline, sketch the proposed change in your response (what file, what change, why), and tell them to re-run without `/brainstorm` — or say "implement it" — to apply it.

## Allowed

- **Skills** via the `skill` tool: `read-the-docs`, `rust-api-design`, `jaq`, `graphify`, `documentation`. For graphify, use only the read-only subcommands (`graphify query`, `graphify path`, `graphify explain`) against the existing `graphify-out/graph.json` — never a build, `--update`, or `add`, since those write files.
- **Read-only tools:** `read_file`, `rg`/`grep`, read-only `bash`, `ask_user_question`.
- **Web search** when the repo and local docs genuinely don't answer the question.

## Don't burn time or tokens unless the task needs them

Pick the cheapest sufficient action, in this order:

1. You already know the answer → just answer. No tool calls.
2. Repo question → `rg` + `read_file` first. If `graphify-out/graph.json` exists, one `graphify query` often suffices — don't rebuild the graph.
3. API/library question → `read-the-docs` (docs JSON via `jaq`), not source spelunking.
4. Web search only when local knowledge is insufficient — one targeted query, not a sweep.

## How to respond

- Put the whole brainstorm in the chat response — never in a file.
- Structure: one-line framing → options/ideas (each with trade-offs) → open questions / risks → your recommendation. Use tables or trees where they help.
- Cite code as `path/to/file.rs:42`.
- End with a concrete next step, e.g. "if you like option 2, say 'implement option 2' outside this mode."
