#!/usr/bin/env bash
#
# Regenerate the whole real-PDF fixture corpus from the PostScript
# sources next to this script.
#
# Ghostscript is a developer-only dependency: the generated ".pdf" files
# are committed, and the test suite reads them directly. CI never runs
# this script.
#
# Usage: "crates/borax-pdf/tests/corpus/generate.sh" (from any directory)

set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"

if ! command -v gs >/dev/null 2>&1; then
	printf '%s\n' \
		'error: ghostscript is required to regenerate the fixture corpus' \
		'       but no "gs" was found on PATH.' \
		'' \
		'The committed .pdf fixtures are what the tests read; you only' \
		'need ghostscript when changing the .ps sources.' >&2
	exit 1
fi

# CompatibilityLevel 1.4 throughout: it keeps the object structure
# uncompressed and readable, and it is the newest level whose encryption
# is RC4, which is what the encrypted fixtures need.
common=(
	-dQUIET -dBATCH -dNOPAUSE -dSAFER
	-sDEVICE=pdfwrite
	-dCompatibilityLevel=1.4
)

# Dropping the trailer /ID array, together with the creation dates the
# sources pin, makes pdfwrite output byte-identical between runs, so
# regenerating the corpus shows a diff only where a source changed.
deterministic=(-dOmitID=true)

# RC4-128 with the permission bits a publisher typically sets. The
# encrypted fixtures keep their /ID: the standard security handler
# derives the file key from it, so their bytes differ between runs.
encryption=(
	-dEncryptionR=3 -dKeyLength=128 -dPermissions=-3904
	-sOwnerPassword='borax-owner'
)

# render <source-stem> <output-stem> [extra gs argument ...]
#
# The shared XMP prologue is read ahead of the fixture's own source.
render() {
	local source="$1" output="$2"
	shift 2
	gs "${common[@]}" "$@" -sOutputFile="$here/$output.pdf" \
		"$here/xmp-prologue.ps" "$here/$source.ps"
}

# render_without_prologue <source-stem> <output-stem> [extra argument ...]
#
# For the one fixture that supplies its own XMP packet.
render_without_prologue() {
	local source="$1" output="$2"
	shift 2
	gs "${common[@]}" "$@" -sOutputFile="$here/$output.pdf" "$here/$source.ps"
}

render 'publisher-info-doi' 'publisher-info-doi' "${deterministic[@]}"
render_without_prologue 'publisher-xmp-doi' 'publisher-xmp-doi' "${deterministic[@]}"
render 'publisher-text-doi' 'publisher-text-doi' "${deterministic[@]}"
render 'arxiv-new-id' 'arxiv-new-id' "${deterministic[@]}"
render 'arxiv-old-id' 'arxiv-old-id' "${deterministic[@]}"
render 'doi-on-third-page' 'doi-on-third-page' "${deterministic[@]}"
render 'doi-past-page-range' 'doi-past-page-range' "${deterministic[@]}"
render 'no-identifier' 'no-identifier' "${deterministic[@]}"

# Permissions-only encryption: an owner password restricts what a viewer
# offers to do, while the empty user password leaves the document
# readable without asking for anything.
render 'encrypted' 'encrypted-owner-only' "${encryption[@]}"

# A non-empty user password: the document cannot be opened at all
# without it.
render 'encrypted' 'encrypted-user-password' "${encryption[@]}" \
	-sUserPassword='secret'

# Rasterise rather than typeset, so the page has no text layer.
gs -dQUIET -dBATCH -dNOPAUSE -dSAFER \
	-sDEVICE=pdfimage24 -r100 \
	-sOutputFile="$here/no-text-layer.pdf" \
	"$here/no-text-layer.ps"

# Truncation at seven tenths lands inside the page content, well before
# the cross-reference table and trailer, so nothing can locate the
# objects that remain.
truncate_fixture() {
	local source="$here/$1.pdf" output="$here/$2.pdf" size
	size="$(wc -c <"$source")"
	head -c "$((size * 7 / 10))" "$source" >"$output"
}

truncate_fixture 'publisher-info-doi' 'malformed-truncated'

printf 'regenerated %s fixtures in %s\n' \
	"$(find "$here" -maxdepth 1 -name '*.pdf' | wc -l)" "$here"
