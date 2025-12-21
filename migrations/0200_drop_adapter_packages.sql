-- Migration 0200: Remove adapter packages feature
-- The adapter_packages layer is unused - strengths are stored but never used by router
-- Stacks remain as the core unit for adapter grouping

DROP TABLE IF EXISTS tenant_package_installs;
DROP TABLE IF EXISTS adapter_packages;
