#!/usr/bin/env python3
import os
import re
import sys

def resolve_merge_conflict_safe(content):
    """
    Safely resolve merge conflicts by keeping the HEAD version.
    This properly handles the file structure by only replacing conflict blocks.
    """
    # Split content into lines
    lines = content.split('\n')
    result = []
    in_conflict = False
    head_content = []
    integration_content = []
    conflict_start = -1
    
    i = 0
    while i < len(lines):
        line = lines[i]
        
        if line.startswith('<<<<<<< HEAD'):
            in_conflict = True
            conflict_start = len(result)
            head_content = []
            integration_content = []
            i += 1
            continue
        elif line.startswith('======='):
            # Switch to integration branch content
            i += 1
            continue
        elif line.startswith('>>>>>>> integration-branch'):
            # End of conflict, choose HEAD content
            result.extend(head_content)
            in_conflict = False
            i += 1
            continue
        
        if in_conflict:
            if not integration_content:  # Still in HEAD section
                head_content.append(line)
            else:  # In integration section
                integration_content.append(line)
        else:
            result.append(line)
        
        i += 1
    
    return '\n'.join(result)

def process_file_safe(filepath):
    """Process a single file to resolve merge conflicts safely."""
    try:
        with open(filepath, 'r', encoding='utf-8') as f:
            content = f.read()
        
        if '<<<<<<< HEAD' not in content:
            return False  # No conflicts in this file
        
        resolved = resolve_merge_conflict_safe(content)
        
        with open(filepath, 'w', encoding='utf-8') as f:
            f.write(resolved)
        
        return True
    except Exception as e:
        print(f"Error processing {filepath}: {e}", file=sys.stderr)
        return False

def main():
    """Main function to process all files with merge conflicts."""
    # Find all files with merge conflicts
    result = os.popen('find . -type f \( -name "*.tsx" -o -name "*.ts" -o -name "*.rs" -o -name "*.sql" -o -name "*.sh" -o -name "*.md" \) -exec grep -l "<<<<<<< HEAD" {} \; 2>/dev/null').read()
    
    files = result.strip().split('\n') if result.strip() else []
    files = [f for f in files if f and not f.startswith('./ui/node_modules/')]  # Remove node_modules
    
    if not files:
        print("No files with merge conflicts found.")
        return
    
    print(f"Found {len(files)} files with merge conflicts:")
    for f in files[:10]:  # Show first 10
        print(f"  {f}")
    if len(files) > 10:
        print(f"  ... and {len(files) - 10} more")
    
    processed = 0
    for filepath in files:
        if process_file_safe(filepath):
            processed += 1
            print(f"✓ {filepath}")
    
    print(f"\nProcessed {processed} files.")
    
    # Check if any conflicts remain
    remaining = os.popen('grep -r "<<<<<<< HEAD" --include="*.tsx" --include="*.ts" --include="*.rs" --include="*.sql" --include="*.sh" --include="*.md" 2>/dev/null | grep -v node_modules | wc -l').read().strip()
    print(f"Remaining conflict markers: {remaining}")

if __name__ == "__main__":
    main()
