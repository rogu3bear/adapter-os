# Phase 47 UAT

## Objective

Operators get deterministic release gating outcomes with clear remediation for model readiness, governance blockers, and local build prerequisites.

## UAT Checklist

1. Run `./start preflight` with missing model path and confirm fail-fast, actionable output.
2. Run `./aosctl --rebuild --help` with invalid `DATABASE_URL` and confirm offline fallback path.
3. Run local release gate with default settings and confirm governance is enforced.
4. Run planning health and confirm no phase-47 roadmap/directory drift warnings.

## Acceptance

`pending`

