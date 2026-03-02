#!/bin/bash
# Visual page inspection script - checks for common errors in HTML/JS

set -e

BASE_URL="${AOS_BASE_URL:-http://localhost:8080}"
PAGES=(
    "/"
    "/dashboard"
    "/adapters"
    "/chat"
    "/system"
    "/settings"
    "/models"
    "/training"
    "/stacks"
    "/collections"
    "/documents"
    "/datasets"
    "/admin"
    "/audit"
    "/workers"
    "/monitoring"
    "/routing"
    "/repositories"
)

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

check_page() {
    local path="$1"
    local url="${BASE_URL}${path}"
    local errors=0
    
    echo -e "\n${CYAN}Checking: ${path}${NC}"
    
    # Get HTML content
    local html
    html=$(curl -s "$url" 2>&1)
    local status_code
    status_code=$(curl -s -o /dev/null -w "%{http_code}" "$url")
    
    if [ "$status_code" != "200" ]; then
        echo -e "  ${RED}✗ HTTP $status_code${NC}"
        return 1
    fi
    
    # Check for common errors
    local issues=()
    
    # Check for JavaScript errors in HTML comments
    if echo "$html" | grep -qi "error\|exception\|failed"; then
        issues+=("Possible error text found")
    fi
    
    # Check for WASM module
    if ! echo "$html" | grep -q "adapteros-ui.*\.wasm"; then
        issues+=("WASM module reference missing")
    fi
    
    # Check for CSS files
    if ! echo "$html" | grep -q "\.css"; then
        issues+=("CSS files missing")
    fi
    
    # Check for title
    if ! echo "$html" | grep -qi "<title>"; then
        issues+=("Title tag missing")
    fi
    
    # Check for viewport meta
    if ! echo "$html" | grep -qi "viewport"; then
        issues+=("Viewport meta tag missing")
    fi
    
    if [ ${#issues[@]} -eq 0 ]; then
        echo -e "  ${GREEN}✓ Page loads correctly${NC}"
        return 0
    else
        echo -e "  ${YELLOW}⚠ Issues found:${NC}"
        for issue in "${issues[@]}"; do
            echo -e "    - $issue"
        done
        return 1
    fi
}

echo ""
echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${CYAN}  Page Inspection - Visual Error Detection${NC}"
echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""
echo "Base URL: $BASE_URL"
echo ""

TOTAL=0
PASSED=0
FAILED=0

for page in "${PAGES[@]}"; do
    if check_page "$page"; then
        ((PASSED++))
    else
        ((FAILED++))
    fi
    ((TOTAL++))
done

echo ""
echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${CYAN}  Summary${NC}"
echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""
echo -e "${GREEN}Passed: $PASSED${NC}"
echo -e "${RED}Failed: $FAILED${NC}"
echo -e "Total: $TOTAL"
echo ""
echo "For detailed visual inspection, open pages in browser and check:"
echo "  1. Browser DevTools Console (F12) for JavaScript errors"
echo "  2. Network tab for failed resource loads"
echo "  3. Visual rendering and layout"
echo "  4. Interactive elements (buttons, forms, links)"
