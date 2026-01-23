# adapterOS TUI Perfect Alignment Specification

## 📐 Canonical Column Alignment Rules

### Core Principle
Every vertical separator (`│`) MUST align perfectly across ALL rows in a component.

## Fixed Column Widths

### Status Bar
```
Static Section (65% of width):
┌──────────────────┬─────────────────┬──────────┐
│ Model: {:<22}    │ Status: {:<15} │ Mode: {:<4} │
└──────────────────┴─────────────────┴──────────┘
Column 1: 30 chars  Column 2: 25 chars  Column 3: 15 chars

Live Data Section (35% of width):
┌────────┬───────────┬─────────┬────────────┐
│ ▣ LIVE │ Mem: {:<3}% │ Q: {:<2} │ TPS: {:<4} │
└────────┴───────────┴─────────┴────────────┘
Fixed positions for perfect alignment
```

### System Status Table
```
┌──────┬───────────────┬─────────────────┬─────────────────────────┐
│Status│ Service Name   │ State          │ Additional Info          │
├──────┼───────────────┼─────────────────┼─────────────────────────┤
│ [OK] │ Database      │ Connected      │ Latency: 1.2ms          │
│ [OK] │ Router        │ Ready          │ Adapters: 12/50         │
│ [!!] │ Security      │ DEVELOPMENT    │ Relaxed policies        │
└──────┴───────────────┴─────────────────┴─────────────────────────┘
Col 1: 6  Col 2: 15  Col 3: 15  Col 4: Variable
```

### Service List Table
```
┌───────┬────────┬──────────────────────┬───────────┬─────────────────┬──────────┐
│Select │ Status │ Service Name         │ State     │ Dependencies    │ Action   │
├───────┼────────┼──────────────────────┼───────────┼─────────────────┼──────────┤
│  >    │ [OK]   │ Database             │ Running   │ None            │ [Restart]│
│       │ [--]   │ Metrics System       │ Stopped   │ None            │ [Start]  │
└───────┴────────┴──────────────────────┴───────────┴─────────────────┴──────────┘
Col 1: 3  Col 2: 6  Col 3: 20  Col 4: 11  Col 5: 17  Col 6: 10
```

### Main Menu
```
┌────┬──────────────────────────────┬──────────────────────────┐
│Sel │ Menu Item                    │ Status                   │
├────┼──────────────────────────────┼──────────────────────────┤
│ >  │ Boot All Services            │ [Ready to boot]          │
│    │ Boot Single Service          │ [Select from list]       │
│    │ Debug Service                │ [All services healthy]   │
└────┴──────────────────────────────┴──────────────────────────┘
Col 1: 3  Col 2: 25  Col 3: 25
```

## Alignment Rules

### 1. Fixed Width Format Strings
Always use format strings with explicit widths:
```rust
// GOOD
format!("{:<20}", text)  // Left-aligned, 20 chars
format!("{:>10}", num)   // Right-aligned, 10 chars
format!("{:^15}", text)  // Center-aligned, 15 chars

// BAD
format!("{}", text)      // Variable width
```

### 2. Vertical Separator Alignment
All `│` characters must be at the same column position across rows:
```
// GOOD - Separators align at columns 23 and 39
Row 1: ║ Model: llama-7b-lora   │ Status: [OK]     │ Mode: DEV  ║
Row 2: ║                        │                  │            ║
       ↑ Column 23              ↑ Column 39

// BAD - Separators don't align
Row 1: ║ Model: llama │ Status: [OK] │ Mode: DEV ║
Row 2: ║ │ │ ║  (misaligned!)
```

### 3. Padding Rules
```rust
// Pad text to fixed width
let padded = format!("{:<20}", text.chars().take(20).collect::<String>());

// Ensure minimum width with truncation
let fixed = if text.len() > 20 {
    format!("{:.20}", text)
} else {
    format!("{:<20}", text)
};
```

### 4. Border Characters
Use consistent box-drawing characters:
```
Outer borders: ╔ ═ ╗ ║ ╚ ╝
Inner dividers: ├ ─ ┤ │ ┬ ┴ ┼
Thick dividers: ╠ ═ ╣ ║ ╬
```

## Testing Alignment

### Visual Test
Run `cargo run --example aligned_example` to see perfect alignment examples.

### Alignment Checker
```rust
fn check_alignment(lines: &[String]) -> bool {
    // Find all │ positions in first line
    let separators: Vec<usize> = lines[0]
        .chars()
        .enumerate()
        .filter(|(_, c)| *c == '│')
        .map(|(i, _)| i)
        .collect();

    // Check all lines have separators at same positions
    lines.iter().all(|line| {
        separators.iter().all(|&pos| {
            line.chars().nth(pos) == Some('│')
        })
    })
}
```

## Common Mistakes to Avoid

### 1. Dynamic Width Based on Content
```rust
// BAD - Width changes with content
format!("{} │ {}", name, status)

// GOOD - Fixed width regardless of content
format!("{:<20} │ {:<15}", name, status)
```

### 2. Mixing Tabs and Spaces
```rust
// BAD - Tabs can have different widths
format!("\t{}\t{}", col1, col2)

// GOOD - Use spaces with fixed counts
format!("{:20} {:15}", col1, col2)
```

### 3. Unicode Width Issues
```rust
// Some unicode characters are wider
// Use unicode_width crate for accurate measurement
use unicode_width::UnicodeWidthStr;

let width = text.width();  // Accurate unicode width
let padded = format!("{:width$}", text, width = 20);
```

## Implementation Checklist

- [x] Status bar columns align between rows
- [x] Service table columns have fixed widths
- [x] Main menu items align properly
- [x] System status uses consistent separators
- [x] All vertical bars `│` align across rows
- [x] Box borders align at corners
- [x] Live data box separate from static data
- [x] No variable-width formatting
- [x] Unicode characters handled properly
- [x] Examples demonstrate perfect alignment

## Summary

Perfect alignment requires:
1. **Fixed column widths** for all text
2. **Consistent separator positions** across rows
3. **Proper padding** to maintain widths
4. **No dynamic sizing** based on content
5. **Testing** with various data to ensure consistency
