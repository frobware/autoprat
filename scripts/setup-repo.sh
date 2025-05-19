#!/usr/bin/env bash

set -e

owner=$1
repo=$2

if [ -z "$owner" ] || [ -z "$repo" ]; then
    echo "Usage: ./setup-repo.sh owner repo"
    exit 1
fi

echo "Setting up repository $owner/$repo..."

echo "Enabling GitHub Actions..."
gh api --method PUT "repos/$owner/$repo/actions/permissions" \
   --field enabled=true \
   --field allowed_actions="all"

echo "GitHub Actions have been enabled for $owner/$repo"
echo ""
echo "NOTE: For branch protection rules, please set them up manually:"
echo "1. Go to the repository settings on GitHub"
echo "2. Navigate to Branches > Branch protection rules"
echo "3. Add a rule for the 'main' branch"
echo "4. Check 'Require status checks to pass before merging'"
echo "5. Search for and select the workflow checks: build, fmt, deps"
echo "6. Save the rule"
