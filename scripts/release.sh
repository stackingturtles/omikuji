#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/.."

echo "🎲 Omikuji Core Release"
echo "======================="
echo ""

if ! git diff --quiet HEAD 2>/dev/null; then
    echo "⚠️  You have uncommitted changes:"
    git status --short
    echo ""
    read -p "Continue anyway? (y/N) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        echo "Aborted."
        exit 1
    fi
fi

echo "Recent tags:"
git tag --sort=-version:refname | head -5
echo ""

read -p "Enter version tag (e.g., v0.7.0): " VERSION

if [[ ! $VERSION =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-.*)?$ ]]; then
    echo "❌ Invalid version format. Must match v*.*.* or v*.*.*-*"
    exit 1
fi

if git rev-parse "$VERSION" >/dev/null 2>&1; then
    echo "❌ Tag $VERSION already exists"
    exit 1
fi

read -p "Enter release message: " MESSAGE

if [[ -z "$MESSAGE" ]]; then
    echo "❌ Message cannot be empty"
    exit 1
fi

echo ""
echo "📋 Summary:"
echo "   Tag:     $VERSION"
echo "   Message: $MESSAGE"
echo "   Branch:  $(git branch --show-current)"
echo "   Commit:  $(git rev-parse --short HEAD)"
echo ""

read -p "Create and push tag? (y/N) " -n 1 -r
echo

if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "Aborted."
    exit 1
fi

echo ""
echo "🏷️  Creating tag $VERSION..."
git tag -a "$VERSION" -m "$MESSAGE"

echo "🚀 Pushing tag to origin..."
git push origin "$VERSION"

echo ""
echo "✅ Done! Tag $VERSION pushed to GitHub."
echo ""
echo "📦 GitHub Actions will now build and push to ECR Public:"
echo "   https://github.com/stackingturtles/omikuji/actions"
echo ""
echo "🐳 Image will be available at:"
echo "   public.ecr.aws/stackingturtles/omikuji/omikuji-core:$VERSION"
