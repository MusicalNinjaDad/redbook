---
name: read-the-docs
description: Load this skill when you need to access Rust crate documentation (JSON format) or standard library documentation (HTML format) for this project.
---

# Read the Docs

This skill provides access to comprehensive Rust documentation for the redbook crate and all its dependencies, as well as the nightly Rust standard library.

## Documentation Sources

### 1. Crate Documentation (JSON Format)

Full documentation for this crate and all dependencies is available in JSON format.

**Target-independent libraries:** `./docs/doc/*.json`

**Target-specific libraries (with target specific cfgs):** `./docs/<target>/doc/*.json`
- Windows MSVC: `./docs/x86_64-pc-windows-msvc/doc/*.json`
- Linux GNU: `./docs/x86_64-unknown-linux-gnu/doc/*.json`

Each JSON file contains:
- Complete API documentation including private items for this project's crate(s)
- Item descriptions, signatures, and documentation strings
- Cross-references and links between items
- Span information (source file locations)
- Attribute information

**Main crate:** `./docs/x86_64-pc-windows-msvc/doc/redbook.json`

**Dependencies:** Individual JSON files for each dependency in the same directories.

### 2. Standard Library Documentation (HTML Format)

Full documentation for the installed nightly version of the Rust standard library is available in HTML format.

**Location:** `/opt/rustup/toolchains/nightly-x86_64-unknown-linux-gnu/share/doc/rust/html/`

**Structure:**
- `alloc/` - Allocator and collection types
- `core/` - Core library (no_std compatible)
- `std/` - Standard library
- `index.html` - Main entry point with search
- Various guides in the root (e.g., `guide-ownership.html`, `guide-macros.html`)

## Indexes

Pre-built indexes are available at `./docs/index/` to speed up documentation queries:

### 1. Name-to-File Index: `./docs/index/name_to_file.json`

Maps item names to their containing file and crate. Structure:
```json
{
  "<item_name>": [
    {"name": "<item_name>", "crate": "<crate_name>", "file": "<filename.json>"},
    ...
  ]
}
```

**Usage:**
```bash
# Find which file contains "MyStruct"
rg '"MyStruct"' ./docs/index/name_to_file.json

# Get the file path for an item using jaq
jaq '.[] | select(.name == "MyStruct") | .file' ./docs/index/name_to_file.json
```

### 2. Name-to-ID Index (Per Crate): `./docs/index/*_name_to_id.json`

Each crate has its own index mapping item names to their internal IDs. Filenames follow the pattern `<crate>_name_to_id.json`.

Structure:
```json
{
  "<item_name>": "<item_id>"
}
```

**Usage:**
```bash
# Find the ID for "Parser" in the syn crate
jaq '."Parser"' ./docs/index/syn_name_to_id.json

# Find the ID for "MyType" in redbook
jaq '."MyType"' ./docs/index/redbook_name_to_id.json
```

### 3. Combined Index: `./docs/index/combined.json`

Comprehensive index containing all items across all crates with full metadata. Structure:
```json
[
  {
    "name": "<item_name>",
    "type": "struct"|"enum"|"trait"|"function"|"module"|"proc_macro",
    "crate": "<crate_name>",
    "file": "<filename.json>",
    "id": "<item_id>",
    "docs": "<documentation_string>" | null
  },
  ...
]
```

**Usage:**
```bash
# Find all items named "Builder"
rg -j '"name": "Builder"' ./docs/index/combined.json

# Get all structs using jaq
jaq '.[] | select(.type == "struct") | {name: .name, crate: .crate}' ./docs/index/combined.json

# Find items by type and name pattern
jaq '.[] | select(.type == "function" and (.name | test("^parse"; "i"))) | {name: .name, crate: .crate, docs: .docs}' ./docs/index/combined.json

# Get documentation for a specific item
jaq '.[] | select(.name == "MyStruct" and .crate == "redbook") | .docs' ./docs/index/combined.json
```

## Accessing Documentation

### Using the Indexes (Preferred)

**ALWAYS load the `jaq` skill first** when working with JSON documentation.

**Find an item and get its full documentation:**
```bash
# Step 1: Find the item in the combined index
ITEM=$(jaq -r '.[] | select(.name == "MyStruct" and .crate == "redbook") | .file + ":" + .id' ./docs/index/combined.json)

# Step 2: Extract file and ID
FILE=$(echo "$ITEM" | cut -d: -f1)
ID=$(echo "$ITEM" | cut -d: -f2)

# Step 3: Get full details
jaq ".index[\"$ID\"]" "$FILE"
```

**Get all public functions from a specific crate:**
```bash
# Using combined index
jaq '.[] | select(.crate == "redbook" and .type == "function") | {name: .name, docs: .docs}' ./docs/index/combined.json
```

**Search by name pattern across all crates:**
```bash
# Find all items matching a pattern
rg -j '"name": "[Ss]erialize' ./docs/index/combined.json

# Then use jaq to get full details for specific matches
```

### Direct JSON Queries

When indexes don't cover your use case, query the JSON files directly.

**List all items in a crate's documentation:**
```bash
jaq '.index | keys[]' ./docs/x86_64-pc-windows-msvc/doc/redbook.json
```

**Get details for a specific item by ID:**
```bash
jaq '.index["123"]' ./docs/x86_64-pc-windows-msvc/doc/redbook.json
```

**Find an item by name in a specific file:**
```bash
jaq '.index | to_entries[] | select(.value.name == "MyStruct")' ./docs/x86_64-pc-windows-msvc/doc/redbook.json
```

**Search for items matching a name pattern:**
```bash
jaq '.index | to_entries[] | select(.value.name | test("^My")) | {id: .key, name: .value.name, kind: .value.inner | keys[0]}' ./docs/x86_64-pc-windows-msvc/doc/redbook.json
```

### Searching Across All Dependencies

**Find all items named "Parser" across all dependencies:**
```bash
rg -j '"name": "Parser"' ./docs/x86_64-pc-windows-msvc/doc/*.json ./docs/x86_64-unknown-linux-gnu/doc/*.json
```

**Get all public functions from all crates:**
```bash
rg -j '.visibility.*"public".*"function"' ./docs/x86_64-pc-windows-msvc/doc/*.json ./docs/x86_64-unknown-linux-gnu/doc/*.json | jaq '.index[].value | {name: .name, file}'
```

### Accessing Standard Library HTML Docs

**Search HTML docs with ripgrep:**
```bash
# Find HTML files mentioning "Iterator"
rg -i "Iterator" /opt/rustup/toolchains/nightly-x86_64-unknown-linux-gnu/share/doc/rust/html/std/ --type html | head -20

# Find the file for std::collections::HashMap
find /opt/rustup/toolchains/nightly-x86_64-unknown-linux-gnu/share/doc/rust/html/std -name "*HashMap*" -type f
```

## Updating Documentation

To regenerate all documentation and indexes, run:

```bash
# Generate docs for both targets
cargo doc --document-private-items --output-format json --all-features \
  --target x86_64-pc-windows-msvc \
  --target-dir docs \
  -Z unstable-options

cargo doc --document-private-items --output-format json --all-features \
  --target x86_64-unknown-linux-gnu \
  --target-dir docs \
  -Z unstable-options

# Update indexes
./docs/update-indexes.sh
```

The `update-indexes.sh` script (see below) creates all three index types.

### Index Generation Script: `./docs/update-indexes.sh`

```bash
#!/bin/bash
set -euo pipefail

INDEX_DIR=./docs/index
DOC_DIRS=("./docs/doc" "./docs/x86_64-pc-windows-msvc/doc" "./docs/x86_64-unknown-linux-gnu/doc")

mkdir -p "$INDEX_DIR"

# =============================================================================
# 1. Name-to-File Index
# =============================================================================
echo "Generating name-to-file index..."
rm -f "$INDEX_DIR/name_to_file.tmp"

for doc_dir in "${DOC_DIRS[@]}"; do
  for file in "$doc_dir"/*.json; do
    [ -f "$file" ] || continue
    basename=$(basename "$file" .json)
    
    # Extract crate name from filename or path
    if [ "$basename" = "redbook" ]; then
      crate_name="redbook"
    else
      crate_name=$(jaq -r '.index | to_entries[] | select(.value.name != null) | .value.span.filename | capture("registry/src/index.crates.io-1949cf8c6b5b557f/([^/]+)/") | .[] | select(. != null) | "\1"' "$file" | head -1)
      crate_name=${crate_name:-$basename}
    fi
    
    jaq -c '.index | to_entries[] | select(.value.name != null) | {name: .value.name, crate: $CRATE, file: $FILE} | select(.name != null)' \
      --arg CRATE "$crate_name" \
      --arg FILE "$file" \
      "$file" >> "$INDEX_DIR/name_to_file.tmp"
  done
done

jaq -s '[.[] | group_by(.name) | .[] | {name: .[0].name, entries: map({crate: .crate, file: .file})}] | map({(.name): .entries}) | reduce .[] as $item ({}; . + $item)' "$INDEX_DIR/name_to_file.tmp" > "$INDEX_DIR/name_to_file.json"
rm -f "$INDEX_DIR/name_to_file.tmp"
echo "Name-to-file index generated."

# =============================================================================
# 2. Name-to-ID Index (Per Crate)
# =============================================================================
echo "Generating name-to-ID indexes..."
rm -f "$INDEX_DIR"/*_name_to_id.json

for doc_dir in "${DOC_DIRS[@]}"; do
  for file in "$doc_dir"/*.json; do
    [ -f "$file" ] || continue
    basename=$(basename "$file" .json)
    
    jaq '{
      name_to_id: (.index | to_entries[] | {(.value.name // ""): .key} | reduce .[] as $item ({}; . + $item) | with_entries(select(.key != ""))),
      crate: $CRATE
    } | .name_to_id' \
      --arg CRATE "$basename" \
      "$file" > "$INDEX_DIR/${basename}_name_to_id.json"
    
    echo "  Generated: $INDEX_DIR/${basename}_name_to_id.json"
  done
done
echo "Name-to-ID indexes generated."

# =============================================================================
# 3. Combined Index with Type Information
# =============================================================================
echo "Generating combined index..."
rm -f "$INDEX_DIR/combined.tmp"

for doc_dir in "${DOC_DIRS[@]}"; do
  for file in "$doc_dir"/*.json; do
    [ -f "$file" ] || continue
    basename=$(basename "$file" .json)
    
    if [ "$basename" = "redbook" ]; then
      crate_name="redbook"
    else
      crate_name=$(jaq -r '.index | to_entries[] | select(.value.name != null) | .value.span.filename | capture("registry/src/index.crates.io-1949cf8c6b5b557f/([^/]+)/") | .[] | select(. != null) | "\1"' "$file" | head -1)
      crate_name=${crate_name:-$basename}
    fi
    
    jaq --arg CRATE "$crate_name" --arg FILE "$file" '
      .index | to_entries[] | 
      select(.value.name != null) | {
        name: .value.name,
        type: (.value.inner | keys[0] // "unknown"),
        crate: $CRATE,
        file: $FILE,
        id: .key,
        docs: (.value.docs // null)
      }
    ' "$file" >> "$INDEX_DIR/combined.tmp"
  done
done

jaq -s '.' "$INDEX_DIR/combined.tmp" > "$INDEX_DIR/combined.json"
rm -f "$INDEX_DIR/combined.tmp"
echo "Combined index generated."

echo "All indexes updated successfully."
```

Make it executable:
```bash
chmod +x ./docs/update-indexes.sh
```

## JSON Structure Reference

Each JSON documentation file has the following structure:

```json
{
  "root": <root_item_id>,
  "crate_version": "<version>",
  "includes_private": true/false,
  "index": {
    "<item_id>": {
      "id": <item_id>,
      "crate_id": <crate_id>,
      "name": "<item_name>",
      "span": {
        "filename": "<source_file>",
        "begin": [<line>, <column>],
        "end": [<line>, <column>]
      },
      "visibility": "public"|"crate"|"private",
      "docs": "<documentation_string>",
      "links": {<link_name>: <target_id>},
      "attrs": [<attributes>],
      "deprecation": null|<deprecation_info>,
      "stability": null|<stability_info>,
      "const_stability": null|<const_stability_info>,
      "inner": {<type_specific_data>}
    }
  },
  "paths": {
    "<path_id>": {
      "crate_id": <crate_id>,
      "path": [<path>, <components>],
      "kind": "<item_kind>",
      "..."
    }
  }
}
```

The `inner` field contains type-specific data:
- `module`: { "is_crate": bool, "items": [<item_ids>], "is_stripped": bool }
- `struct`: { "fields": { ... }, "impls": [...] }
- `enum`: { "variants": [...] }
- `function`: { "signature": ..., "decl": { ... } }
- `trait`: { "items": [...], "impls": [...] }
- `proc_macro`: { "kind": "attr"|"derive"|"bang", "helpers": [...] }

## Best Practices

1. **Prefer indexes for known lookups**: Use the combined index or name-to-file index when you know what you're looking for.

2. **Use ripgrep for discovery**: Use `rg` to find candidate items across all files, then use jaq to extract precise information.

3. **Chain with jaq**: Pipe ripgrep output to jaq for structured extraction.

4. **Always use jaq**: Load the `jaq` skill and use it for all JSON parsing tasks.

5. **Document your queries**: Keep a log of useful jaq queries in `docs/useful-queries` for reuse.

## When to Use This Skill

Load this skill when you need to:
- Look up API documentation for this crate or its dependencies
- Search for specific types, functions, or modules
- Understand the structure of a dependency's API
- Find documentation strings or examples
- Navigate cross-references between items
- Access standard library documentation
- Update or regenerate documentation and indexes
