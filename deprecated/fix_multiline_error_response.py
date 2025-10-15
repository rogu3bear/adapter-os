#!/usr/bin/env python3
"""
Fix multi-line ErrorResponse patterns
"""
import re

def main():
    file_path = "crates/adapteros-server-api/src/handlers.rs"
    
    with open(file_path, 'r') as f:
        content = f.read()
    
    # Count initial patterns
    initial_count = len(re.findall(r'Json\(ErrorResponse \{', content))
    print(f"Initial ErrorResponse patterns: {initial_count}")
    
    # Pattern 1: Multi-line with details: None
    pattern1 = r'Json\(ErrorResponse \{\s*error: "([^"]+)".to_string\(\),\s*details: None\s*\}\)'
    replacement1 = r'Json(ErrorResponse::new("\1").with_code("INTERNAL_ERROR"))'
    content = re.sub(pattern1, replacement1, content, flags=re.MULTILINE | re.DOTALL)
    
    # Pattern 2: Multi-line with details: Some(e.to_string())
    pattern2 = r'Json\(ErrorResponse \{\s*error: "([^"]+)".to_string\(\),\s*details: Some\(e\.to_string\(\)\)\s*\}\)'
    replacement2 = r'Json(ErrorResponse::new("\1").with_code("INTERNAL_ERROR").with_string_details(e.to_string()))'
    content = re.sub(pattern2, replacement2, content, flags=re.MULTILINE | re.DOTALL)
    
    # Pattern 3: Multi-line with details: Some("string".to_string())
    pattern3 = r'Json\(ErrorResponse \{\s*error: "([^"]+)".to_string\(\),\s*details: Some\("([^"]+)".to_string\(\)\)\s*\}\)'
    replacement3 = r'Json(ErrorResponse::new("\1").with_code("INTERNAL_ERROR").with_string_details("\2"))'
    content = re.sub(pattern3, replacement3, content, flags=re.MULTILINE | re.DOTALL)
    
    # Count remaining patterns
    remaining_count = len(re.findall(r'Json\(ErrorResponse \{', content))
    print(f"Remaining ErrorResponse patterns: {remaining_count}")
    print(f"Fixed: {initial_count - remaining_count}")
    
    # Write back if changes were made
    if remaining_count < initial_count:
        with open(file_path, 'w') as f:
            f.write(content)
        print(f"Updated {file_path}")
    else:
        print("No changes made")

if __name__ == "__main__":
    main()
