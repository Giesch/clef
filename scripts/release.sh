#!/bin/bash
set -euo pipefail

cd "$(dirname "$(dirname "$(realpath "${BASH_SOURCE[0]}")")")"

METAINFO_FILE="resources/app.metainfo.xml"

if [ "$(git branch --show-current)" != "main" ]; then
    echo "Error: run the release from the main branch." >&2
    exit 1
fi

if ! git diff --quiet || ! git diff --cached --quiet; then
    echo "Error: the working tree has uncommitted changes. Commit or stash them first." >&2
    exit 1
fi

CURRENT_VERSION=$(grep '^version = ' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
echo "Current version: $CURRENT_VERSION"

echo "Enter new version (without 'v' prefix):"
read -r NEW_VERSION

if ! [[ "$NEW_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.]+)?$ ]]; then
    echo "Error: '$NEW_VERSION' is not a semver version (for example 1.0.0 or 1.0.0-beta.1)." >&2
    exit 1
fi

if git rev-parse "v$NEW_VERSION" >/dev/null 2>&1; then
    echo "Error: tag v$NEW_VERSION already exists." >&2
    exit 1
fi

echo "Enter release notes (one item per line, empty line to finish):"
RELEASE_NOTES=()
while IFS= read -r line; do
    [ -z "$line" ] && break
    RELEASE_NOTES+=("$line")
done

if [ ${#RELEASE_NOTES[@]} -eq 0 ]; then
    echo "Error: release notes cannot be empty." >&2
    exit 1
fi

echo ""
echo "Version: $NEW_VERSION"
echo "Release notes:"
for note in "${RELEASE_NOTES[@]}"; do
    echo "  - $note"
done
echo ""

echo "Proceed with release? (y/N)"
read -r CONFIRM
if [[ ! "$CONFIRM" =~ ^[Yy]$ ]]; then
    echo "Release cancelled." >&2
    exit 1
fi

sed -i "0,/^version = /s/^version = .*/version = \"$NEW_VERSION\"/" Cargo.toml

TODAY=$(date +%Y-%m-%d)
NOTES_XML="    <release version=\"$NEW_VERSION\" date=\"$TODAY\">\n      <description>\n"
for note in "${RELEASE_NOTES[@]}"; do
    escaped_note=$(echo "$note" | sed 's/&/\\\&amp;/g; s/</\\\&lt;/g; s/>/\\\&gt;/g; s/|/\\|/g')
    NOTES_XML+="        <p>$escaped_note</p>\n"
done
NOTES_XML+="      </description>\n    </release>"

if grep -q "<releases>" "$METAINFO_FILE"; then
    sed -i "s|<releases>|<releases>\n$NOTES_XML|" "$METAINFO_FILE"
else
    sed -i "s|</component>|  <releases>\n$NOTES_XML\n  </releases>\n</component>|" "$METAINFO_FILE"
fi

cargo check --quiet --workspace

git add Cargo.toml Cargo.lock "$METAINFO_FILE"
git commit -m "release: v$NEW_VERSION"
git tag -a "v$NEW_VERSION" -m "Release v$NEW_VERSION"
git push
git push origin "v$NEW_VERSION"

echo ""
echo "Pushed v$NEW_VERSION. GitHub Actions builds the .deb and the GitHub Release."
