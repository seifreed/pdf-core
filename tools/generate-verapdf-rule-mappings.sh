#!/usr/bin/env bash
set -euo pipefail

corpus_root="${1:?usage: $0 CORPUS_ROOT [OUTPUT]}"
output="${2:-$corpus_root/RULE-MAPPINGS.json}"
verapdf_bin="${VERAPDF_BIN:-verapdf}"

fixture_root="$corpus_root/fixtures/isartor"
test -d "$fixture_root"
command -v "$verapdf_bin" >/dev/null 2>&1 || test -x "$verapdf_bin"
command -v jq >/dev/null 2>&1

temporary_report="$(mktemp)"
trap 'rm -f "$temporary_report"' EXIT

"$verapdf_bin" --format json --flavour 1b --recurse "$fixture_root" \
    > "$temporary_report" || test -s "$temporary_report"

verapdf_version="$($verapdf_bin --version 2>/dev/null | sed -n 's/^veraPDF //p' | head -n 1)"
test -n "$verapdf_version"

source_commit="$(jq -r '.source.commit' "$corpus_root/CORPUS.json")"
test -n "$source_commit" && test "$source_commit" != "null"

jq --arg fixture_root "$fixture_root" \
   --arg verapdf_version "$verapdf_version" \
   --arg source_commit "$source_commit" \
   '
    .report.jobs as $jobs |
    {
      profile: "PDF/A-1b",
      verapdf_version: $verapdf_version,
      source_commit: $source_commit,
      fixture_count: ($jobs | length),
      mappings: ($jobs | map({
        fixture: (.itemDetails.name | sub(($fixture_root + "/"); "")),
        iso_clause: (if (.itemDetails.name | contains("/PDFA-1b/"))
          then (.itemDetails.name
            | capture("/(?<clause>[0-9]+(?:\\.[0-9]+)+) [^/]+/").clause)
          else null end),
        veraPDF_rules: (.validationResult[0].details.ruleSummaries
          | map(select(.ruleStatus == "FAILED")
          | "\(.specification):\(.clause):\(.testNumber)")),
        expected_compliant: .validationResult[0].compliant
      }))
    }
  ' "$temporary_report" > "$output"

test "$(jq -r '.fixture_count' "$output")" -gt 0
test "$(jq -r '.fixture_count == (.mappings | length)' "$output")" = true
test "$(jq -r '[.mappings[].expected_compliant] | all(.[]; . == false)' "$output")" = true
test "$(jq -r '[.mappings[] | select((.veraPDF_rules | length) == 0)] | length' "$output")" -eq 0

printf 'Generated %s mappings for %s fixtures using veraPDF %s\n' \
    "$(jq -r '.mappings | length' "$output")" \
    "$(jq -r '.fixture_count' "$output")" \
    "$verapdf_version"
