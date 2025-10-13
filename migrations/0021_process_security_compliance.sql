-- Migration 0021: Process Security and Compliance
-- Adds tables for security policies, compliance monitoring, audit trails, and access controls
-- Citation: docs/architecture.md, docs/POLICIES.md

-- Process security policies table
CREATE TABLE IF NOT EXISTS process_security_policies (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    policy_name TEXT NOT NULL,
    policy_type TEXT NOT NULL CHECK(policy_type IN ('access_control','network_security','data_protection','encryption','authentication','authorization')),
    description TEXT,
    policy_rules_json TEXT NOT NULL,
    severity_level TEXT NOT NULL CHECK(severity_level IN ('low','medium','high','critical')),
    enforcement_mode TEXT NOT NULL CHECK(enforcement_mode IN ('enforce','warn','monitor')),
    is_active INTEGER NOT NULL DEFAULT 1,
    created_by TEXT REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_security_policies_tenant_id ON process_security_policies(tenant_id);
CREATE INDEX IF NOT EXISTS idx_security_policies_type ON process_security_policies(policy_type);
CREATE INDEX IF NOT EXISTS idx_security_policies_active ON process_security_policies(is_active);

-- Process compliance standards table
CREATE TABLE IF NOT EXISTS process_compliance_standards (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    standard_name TEXT NOT NULL CHECK(standard_name IN ('SOC2','ISO27001','GDPR','HIPAA','PCI_DSS','ITAR','NIST','FISMA','FedRAMP')),
    version TEXT NOT NULL,
    description TEXT,
    requirements_json TEXT NOT NULL,
    controls_json TEXT NOT NULL,
    is_applicable INTEGER NOT NULL DEFAULT 1,
    assessment_frequency_days INTEGER NOT NULL DEFAULT 90,
    last_assessment_at TEXT,
    next_assessment_at TEXT,
    created_by TEXT REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_compliance_standards_tenant_id ON process_compliance_standards(tenant_id);
CREATE INDEX IF NOT EXISTS idx_compliance_standards_name ON process_compliance_standards(standard_name);
CREATE INDEX IF NOT EXISTS idx_compliance_standards_applicable ON process_compliance_standards(is_applicable);

-- Process security audit logs table
CREATE TABLE IF NOT EXISTS process_security_audit_logs (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    worker_id TEXT REFERENCES workers(id) ON DELETE CASCADE,
    event_type TEXT NOT NULL CHECK(event_type IN ('access_granted','access_denied','policy_violation','configuration_change','authentication','authorization','data_access','network_access')),
    event_category TEXT NOT NULL CHECK(event_category IN ('security','compliance','access','data','network','system')),
    severity TEXT NOT NULL CHECK(severity IN ('info','warning','error','critical')),
    event_description TEXT NOT NULL,
    user_id TEXT REFERENCES users(id),
    source_ip TEXT,
    user_agent TEXT,
    request_id TEXT,
    event_data_json TEXT,
    policy_id TEXT REFERENCES process_security_policies(id),
    compliance_standard_id TEXT REFERENCES process_compliance_standards(id),
    timestamp TEXT NOT NULL DEFAULT (datetime('now')),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_security_audit_tenant_id ON process_security_audit_logs(tenant_id, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_security_audit_worker_id ON process_security_audit_logs(worker_id, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_security_audit_event_type ON process_security_audit_logs(event_type);
CREATE INDEX IF NOT EXISTS idx_security_audit_severity ON process_security_audit_logs(severity);
CREATE INDEX IF NOT EXISTS idx_security_audit_user_id ON process_security_audit_logs(user_id);

-- Process access controls table
CREATE TABLE IF NOT EXISTS process_access_controls (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    resource_type TEXT NOT NULL CHECK(resource_type IN ('worker','configuration','template','deployment','log','metric')),
    resource_id TEXT NOT NULL,
    user_id TEXT REFERENCES users(id),
    role TEXT REFERENCES users(role),
    permission TEXT NOT NULL CHECK(permission IN ('read','write','execute','admin','none')),
    granted_by TEXT REFERENCES users(id),
    granted_at TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at TEXT,
    is_active INTEGER NOT NULL DEFAULT 1,
    conditions_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_access_controls_tenant_id ON process_access_controls(tenant_id);
CREATE INDEX IF NOT EXISTS idx_access_controls_resource ON process_access_controls(resource_type, resource_id);
CREATE INDEX IF NOT EXISTS idx_access_controls_user_id ON process_access_controls(user_id);
CREATE INDEX IF NOT EXISTS idx_access_controls_active ON process_access_controls(is_active);

-- Process vulnerability scans table
CREATE TABLE IF NOT EXISTS process_vulnerability_scans (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    scan_type TEXT NOT NULL CHECK(scan_type IN ('container','dependency','configuration','network','code','infrastructure')),
    target_workers_json TEXT,
    scan_config_json TEXT,
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','running','completed','failed','cancelled')),
    started_at TEXT,
    completed_at TEXT,
    vulnerabilities_found INTEGER NOT NULL DEFAULT 0,
    critical_vulnerabilities INTEGER NOT NULL DEFAULT 0,
    high_vulnerabilities INTEGER NOT NULL DEFAULT 0,
    medium_vulnerabilities INTEGER NOT NULL DEFAULT 0,
    low_vulnerabilities INTEGER NOT NULL DEFAULT 0,
    scan_results_json TEXT,
    remediation_plan_json TEXT,
    initiated_by TEXT REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_vulnerability_scans_tenant_id ON process_vulnerability_scans(tenant_id);
CREATE INDEX IF NOT EXISTS idx_vulnerability_scans_type ON process_vulnerability_scans(scan_type);
CREATE INDEX IF NOT EXISTS idx_vulnerability_scans_status ON process_vulnerability_scans(status);

-- Process vulnerability findings table
CREATE TABLE IF NOT EXISTS process_vulnerability_findings (
    id TEXT PRIMARY KEY,
    scan_id TEXT NOT NULL REFERENCES process_vulnerability_scans(id) ON DELETE CASCADE,
    worker_id TEXT REFERENCES workers(id) ON DELETE CASCADE,
    vulnerability_id TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT NOT NULL,
    severity TEXT NOT NULL CHECK(severity IN ('critical','high','medium','low','info')),
    cvss_score REAL,
    cve_id TEXT,
    affected_component TEXT,
    vulnerability_data_json TEXT,
    remediation_steps_json TEXT,
    status TEXT NOT NULL DEFAULT 'open' CHECK(status IN ('open','in_progress','resolved','false_positive','accepted_risk')),
    assigned_to TEXT REFERENCES users(id),
    due_date TEXT,
    resolved_at TEXT,
    resolved_by TEXT REFERENCES users(id),
    resolution_notes TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_vulnerability_findings_scan_id ON process_vulnerability_findings(scan_id);
CREATE INDEX IF NOT EXISTS idx_vulnerability_findings_worker_id ON process_vulnerability_findings(worker_id);
CREATE INDEX IF NOT EXISTS idx_vulnerability_findings_severity ON process_vulnerability_findings(severity);
CREATE INDEX IF NOT EXISTS idx_vulnerability_findings_status ON process_vulnerability_findings(status);

-- Process compliance assessments table
CREATE TABLE IF NOT EXISTS process_compliance_assessments (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    standard_id TEXT NOT NULL REFERENCES process_compliance_standards(id) ON DELETE CASCADE,
    assessment_type TEXT NOT NULL CHECK(assessment_type IN ('self','internal','external','audit')),
    scope_json TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'planned' CHECK(status IN ('planned','in_progress','completed','failed')),
    started_at TEXT,
    completed_at TEXT,
    assessor_id TEXT REFERENCES users(id),
    assessment_results_json TEXT,
    compliance_score REAL,
    non_compliant_items INTEGER NOT NULL DEFAULT 0,
    remediation_plan_json TEXT,
    next_assessment_date TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_compliance_assessments_tenant_id ON process_compliance_assessments(tenant_id);
CREATE INDEX IF NOT EXISTS idx_compliance_assessments_standard_id ON process_compliance_assessments(standard_id);
CREATE INDEX IF NOT EXISTS idx_compliance_assessments_status ON process_compliance_assessments(status);

-- Process compliance findings table
CREATE TABLE IF NOT EXISTS process_compliance_findings (
    id TEXT PRIMARY KEY,
    assessment_id TEXT NOT NULL REFERENCES process_compliance_assessments(id) ON DELETE CASCADE,
    control_id TEXT NOT NULL,
    control_name TEXT NOT NULL,
    requirement TEXT NOT NULL,
    finding_type TEXT NOT NULL CHECK(finding_type IN ('compliant','non_compliant','partially_compliant','not_applicable')),
    severity TEXT NOT NULL CHECK(severity IN ('critical','high','medium','low')),
    description TEXT NOT NULL,
    evidence_json TEXT,
    remediation_steps_json TEXT,
    status TEXT NOT NULL DEFAULT 'open' CHECK(status IN ('open','in_progress','resolved','accepted_risk')),
    assigned_to TEXT REFERENCES users(id),
    due_date TEXT,
    resolved_at TEXT,
    resolved_by TEXT REFERENCES users(id),
    resolution_notes TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_compliance_findings_assessment_id ON process_compliance_findings(assessment_id);
CREATE INDEX IF NOT EXISTS idx_compliance_findings_type ON process_compliance_findings(finding_type);
CREATE INDEX IF NOT EXISTS idx_compliance_findings_severity ON process_compliance_findings(severity);
CREATE INDEX IF NOT EXISTS idx_compliance_findings_status ON process_compliance_findings(status);
