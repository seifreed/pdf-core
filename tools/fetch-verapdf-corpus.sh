#!/usr/bin/env bash
set -euo pipefail

repo_url="${PDF_CORPUS_REPO_URL:-https://github.com/seifreed/pdf-core-corpus.git}"
commit="${PDF_CORPUS_COMMIT:-c570bd493d717417ee4b805c2cccd41ca1ad0972}"
source_path="${1:-fixtures}"
destination="${2:-.external-corpus/verapdf}"
max_files="${MAX_FILES:-0}"

temporary_dir="$(mktemp -d)"
trap 'rm -rf "$temporary_dir"' EXIT

repo_dir="$temporary_dir/repo"
git clone --filter=blob:none --no-checkout --depth 1 "$repo_url" "$repo_dir"
git -C "$repo_dir" fetch --depth 1 origin "$commit"
git -C "$repo_dir" sparse-checkout init --no-cone
git -C "$repo_dir" sparse-checkout set --no-cone "$source_path"
git -C "$repo_dir" checkout --detach "$commit"

source_dir="$repo_dir/$source_path"
test -d "$source_dir"
rm -rf "$destination"
mkdir -p "$destination"

count=0
while IFS= read -r -d '' source_file; do
    if [[ "$max_files" != 0 && "$count" -ge "$max_files" ]]; then
        break
    fi
    relative_path="${source_file#"$source_dir"/}"
    destination_file="$destination/$relative_path"
    mkdir -p "$(dirname "$destination_file")"
    cp "$source_file" "$destination_file"
    count=$((count + 1))
done < <(find "$source_dir" -type f -iname '*.pdf' -print0 | sort -z)

test "$count" -gt 0
(
    cd "$destination"
    find . -type f -iname '*.pdf' -print0 | sort -z | while IFS= read -r -d '' file; do
        if command -v sha256sum >/dev/null 2>&1; then
            sha256sum "$file"
        else
            shasum -a 256 "$file"
        fi
    done
) > "$destination/SHA256SUMS"

cat > "$destination/SOURCE.json" <<EOF
{
  "repository": "$repo_url",
  "commit": "$commit",
  "path": "$source_path",
  "files": $count
}
EOF

printf 'Fetched %s PDFs from %s at %s into %s\n' \
    "$count" "$repo_url" "$commit" "$destination"
