#!/usr/bin/env python3
"""
Documentation Audit Script for adapterOS

Systematically identifies documentation files that should be pruned:
- Untracked files (not in git)
- Files not referenced in docs/README.md
- Files with broken internal links
- Files that don't match codebase intent
- Duplicate/outdated content
"""

import os
import re
import subprocess
from pathlib import Path
from collections import defaultdict
from typing import Dict, List, Set, Tuple
from dataclasses import dataclass

@dataclass
class DocFile:
    path: str
    tracked: bool
    in_readme: bool
    referenced_count: int
    broken_links: List[str]
    last_modified: str = ""
    size: int = 0

def get_all_docs(docs_dir: Path) -> List[Path]:
    """Get all .md files in docs directory."""
    return list(docs_dir.rglob("*.md"))

def get_tracked_files(docs_dir: Path) -> Set[str]:
    """Get all tracked .md files from git."""
    result = subprocess.run(
        ["git", "ls-files", str(docs_dir) + "/*.md", str(docs_dir) + "/**/*.md"],
        capture_output=True,
        text=True,
        cwd=docs_dir.parent
    )
    files = set()
    for line in result.stdout.strip().split("\n"):
        if line and line.endswith(".md"):
            files.add(line)
    return files

def extract_readme_links(readme_path: Path) -> Set[str]:
    """Extract all .md file links from README.md."""
    if not readme_path.exists():
        return set()
    
    content = readme_path.read_text()
    # Match [text](path.md) or [text](path/to/file.md)
    pattern = r'\[([^\]]+)\]\(([^)]+\.md)\)'
    links = set()
    for match in re.finditer(pattern, content):
        link_path = match.group(2)
        # Normalize relative paths
        if not link_path.startswith("http"):
            # Handle relative paths
            if "/" in link_path:
                links.add(link_path)
            else:
                links.add(link_path)
    return links

def find_all_references(docs_dir: Path) -> Dict[str, int]:
    """Count how many times each doc is referenced by other docs."""
    references = defaultdict(int)
    all_docs = get_all_docs(docs_dir)
    
    for doc in all_docs:
        try:
            content = doc.read_text()
            # Find all markdown links
            pattern = r'\[([^\]]+)\]\(([^)]+\.md)\)'
            for match in re.finditer(pattern, content):
                link_path = match.group(2)
                if not link_path.startswith("http"):
                    # Normalize path
                    if link_path.startswith("./"):
                        link_path = link_path[2:]
                    references[link_path] += 1
                    # Also check absolute paths from docs root
                    if "/" in link_path:
                        references[link_path] += 1
        except Exception as e:
            print(f"Error reading {doc}: {e}")
    
    return dict(references)

def check_file_exists(docs_dir: Path, link_path: str, from_file: Path = None) -> bool:
    """Check if a linked file actually exists."""
    # If link is relative and we know the source file, check relative to source first
    if from_file and not link_path.startswith(("http", "/")):
        source_dir = from_file.parent
        
        # Same directory link (e.g., WORKER_CRASH.md from runbooks/README.md)
        if "/" not in link_path or link_path.startswith("./"):
            same_dir_path = source_dir / link_path.replace("./", "")
            if same_dir_path.exists():
                return True
        
        # Parent directory link (e.g., ../OPERATIONS.md from runbooks/README.md)
        if link_path.startswith("../"):
            parent_path = source_dir / link_path
            if parent_path.exists():
                return True
            # Also try from docs root
            relative_from_docs = from_file.relative_to(docs_dir).parent / link_path.replace("../", "")
            if (docs_dir / relative_from_docs).exists():
                return True
    
    # Try relative to docs root
    full_path = docs_dir / link_path
    if full_path.exists():
        return True
    # Try with various prefixes
    for prefix in ["", "./", "../"]:
        test_path = docs_dir / (prefix + link_path)
        if test_path.exists():
            return True
    return False

def find_broken_links(doc_path: Path, docs_dir: Path) -> List[str]:
    """Find broken internal links in a doc file."""
    broken = []
    try:
        content = doc_path.read_text()
        pattern = r'\[([^\]]+)\]\(([^)]+\.md)\)'
        for match in re.finditer(pattern, content):
            link_path = match.group(2)
            if link_path.startswith("http"):
                continue
            if not check_file_exists(docs_dir, link_path, doc_path):
                broken.append(link_path)
    except Exception as e:
        print(f"Error checking links in {doc_path}: {e}")
    return broken

def get_git_last_modified(file_path: Path, repo_root: Path) -> str:
    """Get last modification date from git."""
    try:
        result = subprocess.run(
            ["git", "log", "-1", "--format=%ai", "--", str(file_path.relative_to(repo_root))],
            capture_output=True,
            text=True,
            cwd=repo_root
        )
        if result.returncode == 0 and result.stdout.strip():
            return result.stdout.strip().split()[0]  # Just the date
    except:
        pass
    return "unknown"

def main():
    repo_root = Path(__file__).parent.parent
    docs_dir = repo_root / "docs"
    readme_path = docs_dir / "README.md"
    
    print("=" * 80)
    print("adapterOS Documentation Audit")
    print("=" * 80)
    print()
    
    # Get all docs
    all_docs = get_all_docs(docs_dir)
    print(f"Total .md files found: {len(all_docs)}")
    
    # Get tracked files
    tracked_files = get_tracked_files(docs_dir)
    print(f"Tracked in git: {len(tracked_files)}")
    print(f"Untracked: {len(all_docs) - len(tracked_files)}")
    print()
    
    # Get README links
    readme_links = extract_readme_links(readme_path)
    print(f"Files referenced in README.md: {len(readme_links)}")
    
    # Get all references
    print("Scanning for cross-references...")
    all_references = find_all_references(docs_dir)
    
    # Analyze each file
    doc_files: List[DocFile] = []
    for doc_path in sorted(all_docs):
        rel_path = str(doc_path.relative_to(repo_root))
        tracked = rel_path in tracked_files or any(
            tracked_file.endswith(rel_path) for tracked_file in tracked_files
        )
        
        # Check if in README
        doc_name = doc_path.name
        in_readme = doc_name in readme_links or any(
            link.endswith(doc_name) for link in readme_links
        )
        
        # Count references
        ref_count = all_references.get(doc_name, 0)
        ref_count += all_references.get(rel_path, 0)
        
        # Find broken links
        broken = find_broken_links(doc_path, docs_dir)
        
        # Get metadata
        last_modified = get_git_last_modified(doc_path, repo_root) if tracked else "untracked"
        size = doc_path.stat().st_size
        
        doc_files.append(DocFile(
            path=rel_path,
            tracked=tracked,
            in_readme=in_readme,
            referenced_count=ref_count,
            broken_links=broken,
            last_modified=last_modified,
            size=size
        ))
    
    # Categorize files
    print()
    print("=" * 80)
    print("PRUNING RECOMMENDATIONS")
    print("=" * 80)
    print()
    
    # Category 1: Untracked files (highest priority for deletion)
    untracked = [d for d in doc_files if not d.tracked]
    if untracked:
        print("🔴 CATEGORY 1: UNTRACKED FILES (Delete immediately)")
        print("-" * 80)
        for doc in sorted(untracked, key=lambda x: x.path):
            print(f"  ❌ {doc.path}")
            if doc.broken_links:
                print(f"      Broken links: {len(doc.broken_links)}")
        print()
    
    # Category 2: Tracked but not referenced anywhere
    orphaned = [d for d in doc_files if d.tracked and not d.in_readme and d.referenced_count == 0]
    if orphaned:
        print("🟡 CATEGORY 2: ORPHANED FILES (Not in README, no references)")
        print("-" * 80)
        for doc in sorted(orphaned, key=lambda x: x.path):
            print(f"  ⚠️  {doc.path}")
            print(f"      Last modified: {doc.last_modified}, Size: {doc.size} bytes")
            if doc.broken_links:
                print(f"      Broken links: {len(doc.broken_links)}")
        print()
    
    # Category 3: Files with broken links
    broken_link_docs = [d for d in doc_files if d.broken_links]
    if broken_link_docs:
        print("🟠 CATEGORY 3: FILES WITH BROKEN LINKS (Needs review)")
        print("-" * 80)
        for doc in sorted(broken_link_docs, key=lambda x: len(x.broken_links), reverse=True):
            if doc.tracked:  # Only show tracked files with broken links
                print(f"  🔗 {doc.path}")
                print(f"      Broken links ({len(doc.broken_links)}): {', '.join(doc.broken_links[:3])}")
                if len(doc.broken_links) > 3:
                    print(f"      ... and {len(doc.broken_links) - 3} more")
        print()
    
    # Category 4: Files referenced in README but don't exist
    print("🔵 CATEGORY 4: README REFERENCES TO MISSING FILES")
    print("-" * 80)
    missing_from_readme = []
    for link in readme_links:
        if not check_file_exists(docs_dir, link):
            missing_from_readme.append(link)
    if missing_from_readme:
        for link in sorted(missing_from_readme):
            print(f"  📄 README.md references: {link} (FILE NOT FOUND)")
    else:
        print("  ✅ All README.md references are valid")
    print()
    
    # Summary
    print("=" * 80)
    print("SUMMARY")
    print("=" * 80)
    print(f"Total files: {len(doc_files)}")
    print(f"  - Tracked: {len([d for d in doc_files if d.tracked])}")
    print(f"  - Untracked: {len(untracked)}")
    print(f"  - In README: {len([d for d in doc_files if d.in_readme])}")
    print(f"  - Orphaned (tracked but unreferenced): {len(orphaned)}")
    print(f"  - With broken links: {len(broken_link_docs)}")
    print()
    print("RECOMMENDED ACTIONS:")
    print(f"  1. Delete {len(untracked)} untracked files")
    print(f"  2. Review {len(orphaned)} orphaned files for deletion or integration")
    print(f"  3. Fix broken links in {len(broken_link_docs)} files")
    if missing_from_readme:
        print(f"  4. Fix {len(missing_from_readme)} broken references in README.md")

if __name__ == "__main__":
    main()
