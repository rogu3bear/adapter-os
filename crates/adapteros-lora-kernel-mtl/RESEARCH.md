# Research/Experimental Modules

This document describes experimental modules that are not production-ready.
These modules are included for research purposes and may change significantly.

## Status: NOT PRODUCTION-READY

The following modules are experimental and should not be used in production:

### `ane_acceleration.rs` (~476 LOC)

**Purpose:** Apple Neural Engine acceleration experiments

**Status:** Research only - ANE integration requires significant additional work for:
- Reliable model compilation pipeline
- Proper fallback handling when ANE is unavailable
- Memory management across ANE/GPU boundaries
- Determinism guarantees (ANE does not provide same determinism as GPU)

**Blockers:**
- No test coverage in CI
- No integration with hot-swap workflow
- ANE availability varies by hardware (M1 vs M2 vs Intel)

### `metal3x.rs` (~526 LOC)

**Purpose:** Metal 3.x feature experiments (mesh shaders, bindless resources)

**Status:** Research only - Requires macOS 14+ and specific GPU hardware

**Blockers:**
- Feature-gated behind macOS version checks
- No backward compatibility
- Limited testing across hardware variants
- Performance characteristics not benchmarked

### `vision_kernels.rs` (~472 LOC)

**Purpose:** Vision model kernel experiments for image processing

**Status:** Research only - Vision model support is not part of current product scope

**Blockers:**
- No integration with LoRA routing
- No test coverage
- Memory requirements not validated
- Not used by any production code path

## Policy

These modules are:
1. **Exported** - Available for experimentation but not documented in public API
2. **Not tested** - No CI coverage, may break without notice
3. **Not maintained** - Will be updated opportunistically, not systematically
4. **Not supported** - No guarantees about stability or correctness

## Migration Path

When any of these modules become production-ready, they should:
1. Have comprehensive test coverage
2. Be integrated with the main backend factory
3. Have proper documentation
4. Pass all policy checks (determinism, memory, etc.)
5. Be moved out of the "research" category in lib.rs

## Deletion Policy

These modules may be deleted if:
- They become stale (no updates for 6+ months)
- They block crate improvements
- They add excessive compile time

Last reviewed: 2025-11-29
