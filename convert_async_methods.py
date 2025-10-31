#!/usr/bin/env python3
"""
Script to convert async fn trait methods to impl Future pattern
Usage: python convert_async_methods.py input.rs > output.rs
"""

import re
import sys

def convert_async_method(match):
    """Convert a single async fn method to impl Future pattern"""
    method_content = match.group(0)

    # Extract method signature
    sig_match = re.search(r'async fn ([^(]+)\(([^)]*)\)\s*->\s*([^;{]+)', method_content)
    if not sig_match:
        return method_content

    method_name = sig_match.group(1)
    params = sig_match.group(2)
    return_type = sig_match.group(3).strip()

    # Convert return type
    if 'Result<' in return_type:
        return_type = f'impl std::future::Future<Output = {return_type}> + Send'

    # Build new signature
    new_sig = f'fn {method_name}({params}) -> {return_type}'

    # Find the method body
    body_start = method_content.find('{')
    if body_start == -1:
        return method_content

    body = method_content[body_start:]

    # Wrap body in async block with proper indentation
    indented_body = '\n'.join(['    ' + line if line.strip() else line for line in body.split('\n')])
    new_body = ' {\n    async' + indented_body[1:] + '\n}'

    return f'    {new_sig}{new_body}'

def main():
    if len(sys.argv) != 2:
        print("Usage: python convert_async_methods.py input.rs")
        sys.exit(1)

    input_file = sys.argv[1]

    with open(input_file, 'r') as f:
        content = f.read()

    # Pattern to match async fn methods in impl blocks (with proper indentation)
    pattern = r'    async fn [^{]+{[^}]*}(?:\s*}|\s*$)'

    # Convert all matches
    converted_content = re.sub(pattern, convert_async_method, content, flags=re.DOTALL | re.MULTILINE)

    print(converted_content)

if __name__ == '__main__':
    main()
