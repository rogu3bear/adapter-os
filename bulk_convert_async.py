#!/usr/bin/env python3
"""
Efficient bulk converter for async fn trait methods to impl Future pattern
Handles method signatures and wraps bodies in async blocks with proper indentation
"""

import re

def convert_async_methods(content):
    """Convert all async fn methods in trait implementations to impl Future pattern"""

    # Pattern to match async fn method declarations with their bodies
    # This handles multi-line signatures and bodies
    pattern = r'(\s+)async fn ([^(]+)\(([^)]*)\) -> ([^{]+)\s*{\s*([^}]*)\s*}'

    def replacement(match):
        indent = match.group(1)
        method_name = match.group(2)
        params = match.group(3)
        return_type = match.group(4).strip()
        body = match.group(5)

        # Convert return type
        return_type = re.sub(r'Result<([^>]+)>', r'impl std::future::Future<Output = Result<\1>> + Send', return_type)

        # Process body - add proper indentation for async block
        body_lines = body.split('\n')
        indented_body = '\n'.join([f'{indent}        {line}' if line.strip() else line for line in body_lines])

        return f'{indent}fn {method_name}({params}) -> {return_type} {{\n{indent}    async {{\n{indented_body}\n{indent}    }}\n{indent}}}'

    # Apply conversion
    converted = re.sub(pattern, replacement, content, flags=re.MULTILINE | re.DOTALL)

    return converted

def main():
    # Read UDS implementation
    with open('crates/adapteros-client/src/uds.rs', 'r') as f:
        uds_content = f.read()

    # Convert UDS implementation
    converted_uds = convert_async_methods(uds_content)

    # Write back
    with open('crates/adapteros-client/src/uds.rs', 'w') as f:
        f.write(converted_uds)

    print("✅ UDS implementation async trait conversion completed")

    # Read Native implementation
    try:
        with open('crates/adapteros-client/src/native.rs', 'r') as f:
            native_content = f.read()

        # Convert Native implementation
        converted_native = convert_async_methods(native_content)

        # Write back
        with open('crates/adapteros-client/src/native.rs', 'w') as f:
            f.write(converted_native)

        print("✅ Native implementation async trait conversion completed")
    except FileNotFoundError:
        print("⚠️  Native implementation file not found, skipping")

if __name__ == '__main__':
    main()
