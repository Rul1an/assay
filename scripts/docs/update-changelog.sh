#!/usr/bin/env bash
# Update changelog with recent merged PRs
set -euo pipefail

CHANGELOG="docs/changelog.md"
TEMP_FILE=$(mktemp)

echo "Updating changelog with recent PRs..."

# Get the date of the last changelog update (look for most recent date header)
LAST_UPDATE=$(grep -m1 '^## \[' "$CHANGELOG" 2>/dev/null | sed 's/.*\[\(.*\)\].*/\1/' || echo "2020-01-01")
echo "Last changelog update: $LAST_UPDATE"

# Get recent merged PRs since last update (max 20)
if command -v gh &>/dev/null && [[ -n "${GH_TOKEN:-}" ]]; then
    echo "Fetching recent merged PRs from GitHub..."

    # Get merged PRs, excluding docs-only and dependabot version bumps
    MERGED_PRS=$(gh pr list \
        --state merged \
        --limit 20 \
        --json number,title,mergedAt,author,labels \
        --jq '
            .[] |
            select(.mergedAt > "'"$LAST_UPDATE"'") |
            select(.title | test("^(chore\\(deps\\)|docs:)") | not) |
            "\(.mergedAt | split("T")[0]) | #\(.number) | \(.title) | @\(.author.login)"
        ' 2>/dev/null || echo "")

    if [[ -z "$MERGED_PRS" ]]; then
        echo "No new PRs to add to changelog"
        exit 0
    fi

    # Group PRs by date
    TODAY=$(date +%Y-%m-%d)

    # Check if today's section already exists
    if grep -q "## \[$TODAY\]" "$CHANGELOG"; then
        echo "Today's section already exists, appending..."
    else
        # Create new section header
        {
            echo ""
            echo "## [$TODAY]"
            echo ""
        } > "$TEMP_FILE"
    fi

    # Parse PRs and append to temp file
    echo "$MERGED_PRS" | while IFS='|' read -r _date number title author; do
        # Clean up whitespace
        number=$(echo "$number" | xargs)
        title=$(echo "$title" | xargs)
        author=$(echo "$author" | xargs)

        # Append to temp file
        echo "- $title ($number) $author" >> "$TEMP_FILE"
    done

    # Insert new section after the first heading
    if [[ -s "$TEMP_FILE" ]]; then
        # Find the line number of the first ## heading (after the title)
        INSERT_LINE=$(grep -n '^## \[' "$CHANGELOG" | head -1 | cut -d: -f1)

        if [[ -n "$INSERT_LINE" ]]; then
            # Insert before the first version section
            {
                head -n $((INSERT_LINE - 1)) "$CHANGELOG"
                cat "$TEMP_FILE"
                tail -n +$INSERT_LINE "$CHANGELOG"
            } > "${CHANGELOG}.new"
            mv "${CHANGELOG}.new" "$CHANGELOG"
            echo "Changelog updated with new entries"
        else
            echo "Could not find insertion point in changelog"
        fi
    fi
else
    echo "GitHub CLI not available or GH_TOKEN not set, skipping changelog update"
    echo "To enable: export GH_TOKEN=\${{ secrets.GITHUB_TOKEN }}"
fi

rm -f "$TEMP_FILE"
echo "Changelog update complete"
