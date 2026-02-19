# UI Action Rules

Phase 6 rules for page-level action hierarchy and interaction choice.

## 1) Action Hierarchy

1. Each page has exactly one primary CTA.
2. Keep exactly 2-3 secondary actions visible.
3. Put all other actions in overflow menus or context menus.

## 2) Placement Rules

- Primary CTA: highest-value outcome for the page, visually primary, first in header action order.
- Secondary actions: frequent support actions (refresh, filters, navigational helpers).
- Overflow/context: low-frequency, advanced, or potentially destructive actions.

## 3) Wizard vs Dialog Rule

- Use a wizard when the flow has 3+ steps or branching paths.
- Use a dialog when the flow is 1-2 simple steps with minimal context switching.

Concrete existing examples:
- Wizard: `CreateJobWizard` on `/training` (`crates/adapteros-ui/src/pages/training/wizard.rs`).
- Dialogs: `CreateStackDialog`/`EditStackDialog` (`crates/adapteros-ui/src/pages/stacks/dialogs.rs`), `SeedModelDialog` (`crates/adapteros-ui/src/pages/models.rs`), `RegisterRepositoryDialog` (`crates/adapteros-ui/src/pages/repositories/mod.rs`).

## 4) Current Page Examples

| Page | Primary CTA | Visible secondary actions | Overflow/context |
|---|---|---|---|
| Dashboard (Guided Flow) | `Teach New Skill` | `Refresh`, `View infrastructure` | Additional links/actions |
| Training | `Create Job` | `Refresh` | Filters and advanced readiness controls |
| Restore Points (`/runs`) | Select a restore point (row/detail as the primary flow action) | `Refresh` | Status filter and row-level replay/diff actions |
| Update Center | `Teach New Skill` | `Refresh` | Promotion/rollback controls in detail context |

## 5) Review Checklist

- One primary CTA defined.
- Only 2-3 secondary actions visible.
- Extra actions moved to overflow/context menus.
- Wizard/dialog choice matches the step/branching threshold.
