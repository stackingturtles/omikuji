#!/bin/sh
# Setup git hooks for development

echo "Setting up git hooks..."

# Configure git to use .githooks directory
git config core.hooksPath .githooks

echo "Git hooks configured successfully!"
echo ""
echo "Pre-commit hook will run:"
echo "  - cargo fmt --check (formatting)"
echo "  - cargo clippy (linting)"
echo ""
echo "To bypass hooks (not recommended): git commit --no-verify"
