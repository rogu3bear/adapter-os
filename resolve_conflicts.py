#!/usr/bin/env python3
import os
import re
import sys

def resolve_merge_conflict(content):
    """
    Resolve merge conflicts by keeping the HEAD version.
    This removes all merge conflict markers and keeps only the HEAD content.
    """
    # Pattern to match merge conflict blocks
    pattern = r'<<<<<<< HEAD\n(.*?)\n=======\n(.*?)\n>>>>>>> integration-branch\n'
    
    # Replace with just the HEAD content
    def replace_func(match):
        head_content = match.group(1)
        return head_content
    
    # Apply the replacement
    resolved = re.sub(pattern, replace_func, content, flags=re.DOTALL)
    
    return resolved

def process_file(filepath):
    """Process a single file to resolve merge conflicts."""
    try:
        with open(filepath, 'r', encoding='utf-8') as f:
            content = f.read()
        
        if '<<<<<<< HEAD' not in content:
            return False  # No conflicts in this file
        
        resolved = resolve_merge_conflict(content)
        
        with open(filepath, 'w', encoding='utf-8') as f:
            f.write(resolved)
        
        return True
    except Exception as e:
        print(f"Error processing {filepath}: {e}", file=sys.stderr)
        return False

def main():
    """Main function to process all files with merge conflicts."""
    # Find all files with merge conflicts
    result = os.popen('find . -type f \( -name "*.tsx" -o -name "*.ts" -o -name "*.rs" -o -name "*.sql" -o -name "*.sh" -o -name "*.md" \) -exec grep -l "<<<<<<< HEAD" {} \;').read()
    
    files = result.strip().split('\n') if result.strip() else []
    files = [f for f in files if f]  # Remove empty strings
    
    if not files:
        print("No files with merge conflicts found.")
        return
    
    print(f"Found {len(files)} files with merge conflicts:")
    for f in files:
        print(f"  {f}")
    
    processed = 0
    for filepath in files:
        if process_file(filepath):
            processed += 1
            print(f"✓ {filepath}")
    
    print(f"\nProcessed {processed} files.")
    
    # Check if any conflicts remain
    remaining = os.popen('grep -r "<<<<<<< HEAD" --include="*.tsx" --include="*.ts" --include="*.rs" --include="*.sql" --include="*.sh" --include="*.md" | wc -l').read().strip()
    print(f"Remaining conflict markers: {remaining}")

if __name__ == "__main__":
    main()
