use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Role definition with explicit permissions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoleDefinition {
    pub name: String,
    pub permissions: HashSet<String>,
    pub inherits: Vec<String>,
}

impl RoleDefinition {
    pub fn new(
        name: impl Into<String>,
        permissions: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            name: name.into(),
            permissions: permissions.into_iter().map(|p| p.into()).collect(),
            inherits: Vec::new(),
        }
    }

    pub fn with_inherits(mut self, inherits: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.inherits = inherits.into_iter().map(|i| i.into()).collect();
        self
    }
}

/// Access policy describing required permission for an action.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccessPolicy {
    pub action: String,
    pub required_permission: String,
}

/// Decision returned by the access control manager.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccessDecision {
    pub allowed: bool,
    pub reasons: Vec<String>,
}

/// Multi-tenant access control manager implementing RBAC semantics.
#[derive(Debug, Default)]
pub struct AccessControlManager {
    roles: HashMap<String, RoleDefinition>,
    assignments: HashMap<(String, String), HashSet<String>>, // (user, tenant) -> roles
}

impl AccessControlManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_role(&mut self, role: RoleDefinition) {
        self.roles.insert(role.name.clone(), role);
    }

    pub fn grant_role(&mut self, user: impl Into<String>, tenant: impl Into<String>, role: &str) {
        let user = user.into();
        let tenant = tenant.into();
        let entry = self
            .assignments
            .entry((user, tenant))
            .or_insert_with(HashSet::new);
        entry.insert(role.to_string());
    }

    pub fn revoke_role(&mut self, user: &str, tenant: &str, role: &str) {
        if let Some(entry) = self
            .assignments
            .get_mut(&(user.to_string(), tenant.to_string()))
        {
            entry.remove(role);
        }
    }

    /// Check if a user can perform an action within a tenant.
    pub fn check(&self, user: &str, tenant: &str, policy: &AccessPolicy) -> AccessDecision {
        let mut reasons = Vec::new();
        let Some(roles) = self
            .assignments
            .get(&(user.to_string(), tenant.to_string()))
        else {
            reasons.push(format!("User {} has no roles in tenant {}", user, tenant));
            return AccessDecision {
                allowed: false,
                reasons,
            };
        };

        let mut permissions = HashSet::new();
        for role in roles {
            if let Some(definition) = self.roles.get(role) {
                permissions.extend(definition.permissions.iter().cloned());
                for inherited in &definition.inherits {
                    if let Some(parent) = self.roles.get(inherited) {
                        permissions.extend(parent.permissions.iter().cloned());
                    }
                }
            }
        }

        if permissions.contains(&policy.required_permission) {
            AccessDecision {
                allowed: true,
                reasons,
            }
        } else {
            reasons.push(format!(
                "Missing permission {} for action {}",
                policy.required_permission, policy.action
            ));
            AccessDecision {
                allowed: false,
                reasons,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enforces_multi_tenant_isolation() {
        let mut acm = AccessControlManager::new();
        acm.register_role(RoleDefinition::new("admin", ["write", "read", "delete"]));
        acm.register_role(RoleDefinition::new("viewer", ["read"]));

        acm.grant_role("alice", "tenant-a", "admin");
        acm.grant_role("alice", "tenant-b", "viewer");

        let policy = AccessPolicy {
            action: "delete-adapter".into(),
            required_permission: "delete".into(),
        };

        let decision_a = acm.check("alice", "tenant-a", &policy);
        assert!(decision_a.allowed);

        let decision_b = acm.check("alice", "tenant-b", &policy);
        assert!(!decision_b.allowed);
    }

    #[test]
    fn inheritance_grants_permissions() {
        let mut acm = AccessControlManager::new();
        acm.register_role(RoleDefinition::new("base", ["read"]));
        acm.register_role(RoleDefinition::new("operator", ["write"]).with_inherits(["base"]));
        acm.grant_role("bob", "tenant-x", "operator");

        let policy = AccessPolicy {
            action: "fetch-report".into(),
            required_permission: "read".into(),
        };
        let decision = acm.check("bob", "tenant-x", &policy);
        assert!(decision.allowed);
    }
}
