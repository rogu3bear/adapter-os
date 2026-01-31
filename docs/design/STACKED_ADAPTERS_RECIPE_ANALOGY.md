# Stacked Adapters: Recipe Analogy

A short analogy to explain adapter stacks using cooking.

---

## The Kitchen Setup

| AdapterOS concept | Recipe analogy |
|-------------------|----------------|
| **Base model** | The base recipe (e.g. *vanilla cake batter*). Same foundation every time. |
| **Adapter** | A small add-on or modifier (e.g. *chocolate swirl*, *lemon zest*, *nuts*). Each is a separate “mini-recipe” you can mix in. |
| **Adapter stack** | The full “recipe card” that says: *Start with the base batter, then apply these modifiers in this way.* |
| **Stack hash** | A fingerprint of exactly which modifiers (and which versions) are on the card—so you can reproduce the same dish later. |

---

## What a Stack Is

A **stack** is an ordered list of adapters plus a **workflow type** that says how they’re combined:

- **Sequential** — Do one modifier after another: first chocolate swirl, then lemon zest, then nuts. Order matters.
- **Upstream–downstream** — Same idea as sequential: one step feeds the next (e.g. base → adapter A → adapter B).
- **Parallel** — Treat the modifiers as a single combined mix (conceptually: “add all of these together” in the routing/combination sense the system uses).

So:

- **One adapter** = “Use base cake + chocolate swirl.”
- **Stack of adapters** = “Use base cake + chocolate swirl, then lemon zest, then nuts,” with the workflow type telling the system whether to apply them in sequence or in parallel.

---

## Why Stacks Exist

- **Reuse** — Same “recipe card” (stack) for many requests: e.g. “Company docs stack” = base model + docs adapter + style adapter.
- **Reproducibility** — The stack (and its hash) fix exactly which adapters and versions were used, so the same stack gives the same behavior.
- **Versioning** — You can name and version a stack (e.g. `v1.0.0`) and change the list of adapters over time while keeping a stable “recipe name.”

---

## TL;DR

**Base model** = base recipe. **Adapters** = small modifiers (chocolate, lemon, nuts). **Stack** = the recipe card that says “use this base + these modifiers in this order (or parallel).” **Stack hash** = fingerprint of which modifiers (and versions) are on the card so you can reproduce the same “dish” every time.
