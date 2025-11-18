#!/usr/bin/env python3
"""
Determinism Edge Cases Dataset Generator
Type 5: Router & Kernel Stress Testing

Generates ~200 synthetic adversarial inputs to test deterministic execution:
- Floating-point ambiguities
- Extremely short inputs
- Repeated tokens
- Capitalization cycles
- Punctuation overload
- Repeated identical inputs (critical for determinism verification)
"""

import json
import hashlib
from typing import List, Dict, Any


def generate_floating_point_edge_cases() -> List[Dict[str, Any]]:
    """Generate inputs that stress floating-point determinism."""
    cases = []

    # Very small differences that might round differently
    bases = [
        "0.1 + 0.2",
        "1e-15 + 1e-16",
        "0.9999999999999999",
        "1.0000000000000001",
        "inf",
        "-inf",
        "nan",
        "1.7976931348623157e+308",  # Max float64
        "2.2250738585072014e-308",  # Min positive float64
    ]

    for base in bases:
        cases.append({
            "input": base,
            "metadata": {
                "case": "floating_point_precision",
                "subcategory": "extreme_values",
                "difficulty": "high"
            }
        })

        # Repeated versions to test determinism
        cases.append({
            "input": f"{base} {base} {base}",
            "metadata": {
                "case": "floating_point_repetition",
                "subcategory": "extreme_values",
                "difficulty": "high"
            }
        })

    # Expressions that might have rounding ambiguity
    ambiguous = [
        "What is 0.1 + 0.2 + 0.3?",
        "Calculate: 1e308 * 2",
        "Divide: 1.0 / 3.0 * 3.0",
        "Compare: 0.9999999999999999 vs 1.0",
        "Sum: " + " + ".join(["0.1"] * 10),
    ]

    for expr in ambiguous:
        cases.append({
            "input": expr,
            "metadata": {
                "case": "floating_point_ambiguity",
                "subcategory": "rounding_errors",
                "difficulty": "high"
            }
        })

    return cases


def generate_short_input_edge_cases() -> List[Dict[str, Any]]:
    """Generate extremely short inputs that stress tokenization."""
    cases = []

    # Single characters
    single_chars = ["a", "A", "1", "!", "?", ".", ",", " ", "\n", "\t"]
    for char in single_chars:
        cases.append({
            "input": char,
            "metadata": {
                "case": "single_character",
                "subcategory": "minimal_input",
                "difficulty": "medium"
            }
        })

    # Two characters
    two_chars = ["ab", "AB", "12", "!?", "..", "  ", "\n\n"]
    for chars in two_chars:
        cases.append({
            "input": chars,
            "metadata": {
                "case": "two_characters",
                "subcategory": "minimal_input",
                "difficulty": "medium"
            }
        })

    # Empty and whitespace-only
    whitespace_cases = [
        "",
        " ",
        "  ",
        "   ",
        "\n",
        "\n\n",
        "\t",
        "\t\t",
        " \n ",
        " \t ",
    ]

    for ws in whitespace_cases:
        cases.append({
            "input": ws,
            "metadata": {
                "case": "whitespace_only",
                "subcategory": "minimal_input",
                "difficulty": "high"
            }
        })

    return cases


def generate_repetition_edge_cases() -> List[Dict[str, Any]]:
    """Generate inputs with repeated tokens to test determinism."""
    cases = []

    # Single token repeated (CRITICAL for determinism)
    tokens = ["A", "the", "test", "1", ".", "!"]
    repetitions = [10, 50, 100, 500, 1000]

    for token in tokens:
        for reps in repetitions:
            repeated = (token + " ") * reps
            cases.append({
                "input": repeated.strip(),
                "metadata": {
                    "case": "token_repetition",
                    "subcategory": "repeated_identical_inputs",
                    "token": token,
                    "count": reps,
                    "difficulty": "critical"
                }
            })

    # Same input repeated multiple times (to verify identical outputs)
    identical_inputs = [
        "Hello world",
        "What is 2+2?",
        "The quick brown fox",
        "Test test test",
        "AAAAAAAAAA",
    ]

    for inp in identical_inputs:
        # Add same input 5 times to ensure deterministic routing
        for i in range(5):
            cases.append({
                "input": inp,
                "metadata": {
                    "case": "repeated_identical_input",
                    "subcategory": "determinism_verification",
                    "input_hash": hashlib.sha256(inp.encode()).hexdigest()[:16],
                    "repetition_index": i,
                    "difficulty": "critical"
                }
            })

    return cases


def generate_capitalization_edge_cases() -> List[Dict[str, Any]]:
    """Generate inputs with capitalization cycles to test case sensitivity."""
    cases = []

    base_text = "the quick brown fox jumps over the lazy dog"

    # All lowercase
    cases.append({
        "input": base_text.lower(),
        "metadata": {
            "case": "capitalization_lowercase",
            "subcategory": "case_variation",
            "difficulty": "medium"
        }
    })

    # All uppercase
    cases.append({
        "input": base_text.upper(),
        "metadata": {
            "case": "capitalization_uppercase",
            "subcategory": "case_variation",
            "difficulty": "medium"
        }
    })

    # Title case
    cases.append({
        "input": base_text.title(),
        "metadata": {
            "case": "capitalization_titlecase",
            "subcategory": "case_variation",
            "difficulty": "medium"
        }
    })

    # Alternating case
    alternating = "".join(
        c.upper() if i % 2 == 0 else c.lower()
        for i, c in enumerate(base_text)
    )
    cases.append({
        "input": alternating,
        "metadata": {
            "case": "capitalization_alternating",
            "subcategory": "case_variation",
            "difficulty": "high"
        }
    })

    # Random capitalization patterns
    patterns = [
        "tHe QuIcK bRoWn FoX",
        "THE quick BROWN fox",
        "ThE qUiCk BrOwN fOx",
    ]

    for pattern in patterns:
        cases.append({
            "input": pattern,
            "metadata": {
                "case": "capitalization_random",
                "subcategory": "case_variation",
                "difficulty": "high"
            }
        })

    return cases


def generate_punctuation_edge_cases() -> List[Dict[str, Any]]:
    """Generate inputs with punctuation overload."""
    cases = []

    # Excessive punctuation
    punct_patterns = [
        "!!!!!!!!!!!!",
        "????????????",
        "............",
        ",,,,,,,,,,,",
        ";;;;;;;;;;;",
        ":::::::::::",
        "'''''''''''",
        '"""""""""""',
        "(((((((((((",
        ")))))))))))",
        "[[[[[[[[[[[",
        "]]]]]]]]]]]",
    ]

    for pattern in punct_patterns:
        cases.append({
            "input": pattern,
            "metadata": {
                "case": "punctuation_overload",
                "subcategory": "repeated_punctuation",
                "difficulty": "medium"
            }
        })

    # Mixed punctuation
    mixed = [
        "!?!?!?!?!?",
        ".,.,.,.,.,",
        ";:;:;:;:;:",
        "()()()()())",
        "[]{}[]{}[]",
    ]

    for pattern in mixed:
        cases.append({
            "input": pattern,
            "metadata": {
                "case": "punctuation_mixed",
                "subcategory": "alternating_punctuation",
                "difficulty": "medium"
            }
        })

    # Punctuation with words
    punct_with_words = [
        "Hello!!!!!!",
        "What????????",
        "Test.......",
        "Code,,,,,,,",
        "Data;;;;;;;",
    ]

    for pattern in punct_with_words:
        cases.append({
            "input": pattern,
            "metadata": {
                "case": "punctuation_with_words",
                "subcategory": "excessive_terminal_punctuation",
                "difficulty": "medium"
            }
        })

    return cases


def generate_special_character_edge_cases() -> List[Dict[str, Any]]:
    """Generate inputs with special characters and unicode edge cases."""
    cases = []

    # Unicode edge cases
    unicode_cases = [
        "🔥🔥🔥🔥🔥",
        "→→→→→→→→→→",
        "∞∞∞∞∞∞∞∞∞∞",
        "αβγδεζηθικ",
        "你好世界你好世界",
        "مرحبا بك مرحبا",
        "שלום שלום שלום",
        "🎉🎊🎈🎁🎀",
    ]

    for ucase in unicode_cases:
        cases.append({
            "input": ucase,
            "metadata": {
                "case": "unicode_characters",
                "subcategory": "non_ascii",
                "difficulty": "high"
            }
        })

    # Control characters (escaped)
    control = [
        "\x00" * 10,
        "\x01" * 10,
        "\x1f" * 10,
        "\x7f" * 10,
    ]

    for ctrl in control:
        cases.append({
            "input": ctrl,
            "metadata": {
                "case": "control_characters",
                "subcategory": "non_printable",
                "difficulty": "high"
            }
        })

    return cases


def generate_boundary_edge_cases() -> List[Dict[str, Any]]:
    """Generate boundary condition edge cases."""
    cases = []

    # Very long repeated sequences
    long_sequences = [
        "A" * 1000,
        "1" * 500,
        " " * 1000,
        "test " * 200,
    ]

    for seq in long_sequences:
        cases.append({
            "input": seq,
            "metadata": {
                "case": "boundary_long_sequence",
                "subcategory": "length_extreme",
                "length": len(seq),
                "difficulty": "high"
            }
        })

    # Nested structures
    nested = [
        "(" * 100 + ")" * 100,
        "[" * 50 + "]" * 50,
        "{" * 50 + "}" * 50,
    ]

    for nest in nested:
        cases.append({
            "input": nest,
            "metadata": {
                "case": "boundary_nested_structures",
                "subcategory": "deep_nesting",
                "difficulty": "medium"
            }
        })

    return cases


def generate_numeric_edge_cases() -> List[Dict[str, Any]]:
    """Generate numeric edge cases including overflow, underflow, and special values."""
    cases = []

    # Integer boundaries
    int_boundaries = [
        "2147483647",  # Max int32
        "-2147483648",  # Min int32
        "9223372036854775807",  # Max int64
        "-9223372036854775808",  # Min int64
        "0",
        "-0",
        "00000000001",
        "-00000000001",
    ]

    for num in int_boundaries:
        cases.append({
            "input": num,
            "metadata": {
                "case": "integer_boundaries",
                "subcategory": "numeric_limits",
                "difficulty": "high"
            }
        })

    # Repeated numbers
    for num in ["0", "1", "42", "999"]:
        for count in [10, 50, 100]:
            repeated = (num + " ") * count
            cases.append({
                "input": repeated.strip(),
                "metadata": {
                    "case": "numeric_repetition",
                    "subcategory": "repeated_numbers",
                    "difficulty": "medium"
                }
            })

    return cases


def generate_tokenizer_stress_cases() -> List[Dict[str, Any]]:
    """Generate cases that stress tokenizer boundary conditions."""
    cases = []

    # Mixed alphanumeric
    mixed = [
        "a1b2c3d4e5",
        "test123test456",
        "ABC123DEF456",
        "1a2b3c4d5e",
        "Code2Test3Deploy4",
        "v1.2.3.4.5",
        "user@email.com123",
        "file_name_123.txt",
    ]

    for m in mixed:
        cases.append({
            "input": m,
            "metadata": {
                "case": "mixed_alphanumeric",
                "subcategory": "tokenizer_stress",
                "difficulty": "medium"
            }
        })

    # Repeated patterns
    patterns = [
        "abcabc" * 20,
        "123123" * 20,
        "XYZ" * 30,
        "+-+" * 30,
        "test_" * 25,
        "<<>>" * 25,
    ]

    for pattern in patterns:
        cases.append({
            "input": pattern,
            "metadata": {
                "case": "repeating_pattern",
                "subcategory": "tokenizer_stress",
                "difficulty": "medium"
            }
        })

    # Whitespace variations
    ws_variations = [
        "word\tword\tword",
        "line\nline\nline",
        "space  space   space",
        "mixed \t\n  mixed",
        "\r\n\r\n\r\n",
    ]

    for ws in ws_variations:
        cases.append({
            "input": ws,
            "metadata": {
                "case": "whitespace_variations",
                "subcategory": "tokenizer_stress",
                "difficulty": "medium"
            }
        })

    return cases


def main():
    """Generate all edge case datasets."""
    all_cases = []

    # Generate all categories
    all_cases.extend(generate_floating_point_edge_cases())
    all_cases.extend(generate_short_input_edge_cases())
    all_cases.extend(generate_repetition_edge_cases())
    all_cases.extend(generate_capitalization_edge_cases())
    all_cases.extend(generate_punctuation_edge_cases())
    all_cases.extend(generate_special_character_edge_cases())
    all_cases.extend(generate_boundary_edge_cases())
    all_cases.extend(generate_numeric_edge_cases())
    all_cases.extend(generate_tokenizer_stress_cases())

    # Write to JSONL file
    output_file = "determinism-edge-cases.jsonl"
    with open(output_file, "w", encoding="utf-8") as f:
        for case in all_cases:
            f.write(json.dumps(case, ensure_ascii=False) + "\n")

    # Generate statistics
    print(f"Generated {len(all_cases)} edge case samples")
    print("\nBreakdown by category:")

    categories = {}
    for case in all_cases:
        cat = case["metadata"]["case"]
        categories[cat] = categories.get(cat, 0) + 1

    for cat, count in sorted(categories.items(), key=lambda x: -x[1]):
        print(f"  {cat}: {count}")

    print(f"\nDataset written to: {output_file}")

    # Generate manifest
    manifest = {
        "name": "determinism_edge_cases_v1",
        "description": "Type 5: Determinism Edge Cases - Synthetic adversarial inputs for router and kernel stress testing including floating-point ambiguities, extremely short inputs, repeated tokens, capitalization cycles, punctuation overload, and repeated identical inputs",
        "version": "1.0.0",
        "category": "synthetic",
        "scope": "global",
        "tier": "critical",
        "rank": 4,
        "alpha": 8.0,
        "target_modules": ["gate_proj", "o_proj"],
        "entries": [
            {
                "path": "determinism-edge-cases.jsonl",
                "format": "jsonl",
                "weight": 1.0,
                "role": "adversarial",
                "notes": "Adversarial edge cases for determinism verification: floating-point precision, minimal inputs, token repetition, case variation, punctuation overload, unicode, and boundary conditions"
            }
        ],
        "provenance": {
            "masterplan_sections": [
                "docs/architecture/MasterPlan.md#deterministic-execution-engine",
                "docs/CLAUDE.md#deterministic-executor-seeding"
            ],
            "created_by": "determinism-edge-cases-dataset-generator",
            "created_at": "2025-11-18T00:00:00Z",
            "last_reviewed_at": "2025-11-18T00:00:00Z",
            "review_notes": "Comprehensive adversarial dataset for stress testing deterministic router decisions and kernel execution under edge case conditions"
        },
        "evaluation_gates": [
            "Identical inputs produce identical router decisions 100% of time",
            "Floating-point operations are deterministic across runs",
            "Short inputs (0-2 chars) handle gracefully",
            "Repeated token sequences maintain determinism",
            "Unicode and special characters processed consistently"
        ],
        "intent": "Stress test the deterministic execution guarantees of AdapterOS by exposing router and kernel components to adversarial edge cases that might cause non-deterministic behavior in naive implementations",
        "edge_case_categories": {
            "floating_point_precision": "Inputs that test floating-point rounding and precision determinism",
            "minimal_input": "Extremely short inputs (0-2 characters) that stress tokenization",
            "token_repetition": "Repeated tokens to verify deterministic handling of identical patterns",
            "case_variation": "Capitalization cycles to test case sensitivity consistency",
            "punctuation_overload": "Excessive punctuation to test tokenizer robustness",
            "unicode_characters": "Non-ASCII characters including emoji and international scripts",
            "control_characters": "Non-printable characters that might break tokenization",
            "boundary_conditions": "Length extremes and deep nesting to test limits",
            "repeated_identical_inputs": "CRITICAL - Same input repeated multiple times to verify identical outputs"
        },
        "determinism_verification": {
            "method": "Each input with 'repeated_identical_input' metadata should produce identical router gate activations and kernel outputs across multiple runs",
            "critical_subcategories": ["repeated_identical_inputs", "determinism_verification"],
            "expected_behavior": "Zero divergence in tick ledger entries for identical inputs"
        }
    }

    with open("manifest.json", "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=2, ensure_ascii=False)

    print("Manifest written to: manifest.json")


if __name__ == "__main__":
    main()
