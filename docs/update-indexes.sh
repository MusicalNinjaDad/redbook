#!/bin/bash
set -euo pipefail

INDEX_DIR=./docs/index
DOC_DIRS=("./docs/doc" "./docs/x86_64-pc-windows-msvc/doc" "./docs/x86_64-unknown-linux-gnu/doc")

mkdir -p "$INDEX_DIR"

# Function to extract crate name from filename
get_crate_name() {
  local file="$1"
  local basename
  basename=$(basename "$file" .json)
  
  # For redbook, use the basename directly
  if [ "$basename" = "redbook" ]; then
    echo "redbook"
    return
  fi
  
  # Try to get crate name from the first entry in the index
  local crate_name
  crate_name=$(jaq -r '.index | to_entries[0] | .value.span.filename | split("/") | .[6] | split("-")[0]' "$file" 2>/dev/null || echo "")
  
  # Fallback to basename if extraction fails
  if [ -z "$crate_name" ]; then
    echo "$basename"
  else
    echo "$crate_name"
  fi
}

# =============================================================================
# 1. Name-to-File Index
# =============================================================================
echo "Generating name-to-file index..."
rm -f "$INDEX_DIR/name_to_file.tmp"

for doc_dir in "${DOC_DIRS[@]}"; do
  for file in "$doc_dir"/*.json; do
    [ -f "$file" ] || continue
    
    crate_name=$(get_crate_name "$file")
    
    jaq -c '.index | to_entries[] | select(.value.name != null) | {name: .value.name, crate: $CRATE, file: $FILE} | select(.name != null)' \
      --arg CRATE "$crate_name" \
      --arg FILE "$file" \
      "$file" >> "$INDEX_DIR/name_to_file.tmp"
  done
done

jaq -s 'reduce .[] as $item ({}; .[$item.name] += [{crate: $item.crate, file: $item.file}])' "$INDEX_DIR/name_to_file.tmp" > "$INDEX_DIR/name_to_file.json"
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
      name_to_id: (.index | to_entries | map({(.value.name // ""): .key}) | reduce .[] as $item ({}; . + $item) | with_entries(select(.key != ""))),
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
    
    crate_name=$(get_crate_name "$file")
    
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
