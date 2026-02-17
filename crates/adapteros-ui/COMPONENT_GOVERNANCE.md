# Component Governance

## Purpose and Scope
- Governs UI component decisions in `crates/adapteros-ui`.
- Applies to reusable components, shared controls, and route-level usage.
- Objective: prevent duplication and keep behavior/accessibility consistent.

## Decision Rule: Extend Existing vs Create New
1. Extend an existing primitive/component first when the needed behavior can be expressed with props, variants, composition, or slots.
2. Create a new component only when existing primitives cannot represent the required semantics/interaction cleanly and reuse is expected beyond a single page.
3. If there is uncertainty, default to extension and document why a new component is necessary in the PR.

## Anti-Patterns (Disallowed)
- Copy-pasted wrappers around existing shared components with only renaming or trivial style changes.
- Page-local duplicates of shared UI controls (for example buttons, inputs, tables, modals, tabs, pagination).
- Parallel component implementations for an existing pattern instead of adding a variant to the current primitive.

## Required Checklist for New Component PRs
- [ ] Route impact is identified (which routes are affected and why existing components were insufficient).
- [ ] Accessibility is validated (keyboard flow, focus, labels/roles/ARIA, contrast).
- [ ] Tests are added or updated at the smallest relevant level (component/unit and route integration when behavior changes).
- [ ] Similarity check is run and results reviewed:
```bash
python3 /Users/star/Dev/adapter-os/scripts/ui_component_similarity.py --threshold 0.80 --exclude-file-suffix components/icons.rs --max-qualifying 8
```
- [ ] PR description includes a short "extend vs create" rationale and nearest existing component evaluated.
