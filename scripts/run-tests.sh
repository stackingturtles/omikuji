#!/bin/bash
# Script to run tests with proper database setup

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}=== Running Omikuji Tests ===${NC}"
echo ""

# Setup test database
echo -e "${YELLOW}Setting up test database...${NC}"
./scripts/setup-test-db.sh

# Export test database URL
export DATABASE_URL="postgresql://omikuji:omikuji_password@localhost:5433/omikuji_test"

# Run tests with any additional arguments passed to the script
echo ""
echo -e "${YELLOW}Running tests...${NC}"
if cargo test "$@"; then
    echo ""
    echo -e "${GREEN}✓ All tests passed!${NC}"
    exit 0
else
    echo ""
    echo -e "${RED}✗ Some tests failed!${NC}"
    exit 1
fi