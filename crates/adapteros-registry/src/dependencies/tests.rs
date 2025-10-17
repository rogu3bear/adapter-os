use super::*;
use adapteros_core::{AosError, B3Hash, Result as AosResult};
use adapteros_manifest::AdapterDependencies;
use std::collections::BTreeMap;
use tempfile::tempdir;

fn register(registry: &Registry, id: &str) {
    let hash = B3Hash::hash(id.as_bytes());
    registry
        .register_adapter(id, &hash, "persistent", 1, &[])
        .expect("register adapter");
}

fn make_deps(
    base_model: Option<&str>,
    requires: &[&str],
    conflicts: &[&str],
) -> AdapterDependencies {
    AdapterDependencies {
        base_model: base_model.map(|s| s.to_string()),
        requires_adapters: requires.iter().map(|s| s.to_string()).collect(),
        conflicts_with: conflicts.iter().map(|s| s.to_string()).collect(),
    }
}

fn setup_registry() -> (tempfile::TempDir, Registry) {
    let dir = tempdir().expect("create temp dir for registry");
    let db_path = dir.path().join("registry.db");
    let registry = Registry::open(&db_path).expect("open registry");
    (dir, registry)
}

#[test]
fn resolves_linear_dependency_chain() -> AosResult<()> {
    let (_dir, registry) = setup_registry();
    register(&registry, "adapter_a");
    register(&registry, "adapter_b");
    register(&registry, "adapter_c");

    let resolver = registry.dependency_resolver();

    let mut manifests: BTreeMap<String, AdapterDependencies> = BTreeMap::new();
    manifests.insert(
        "adapter_a".into(),
        make_deps(Some("qwen2.5-7b"), &["adapter_b"], &[]),
    );
    manifests.insert(
        "adapter_b".into(),
        make_deps(Some("qwen2.5-7b"), &["adapter_c"], &[]),
    );
    manifests.insert("adapter_c".into(), make_deps(Some("qwen2.5-7b"), &[], &[]));

    let graph = resolver.resolve(
        "adapter_a",
        manifests.get("adapter_a").unwrap(),
        "qwen2.5-7b",
        |id| Ok(manifests.get(id).cloned()),
    )?;

    assert_eq!(graph.get("adapter_a"), Some(&vec!["adapter_b".to_string()]));
    assert_eq!(graph.get("adapter_b"), Some(&vec!["adapter_c".to_string()]));
    assert_eq!(graph.get("adapter_c"), Some(&Vec::<String>::new()));

    Ok(())
}

#[test]
fn rejects_missing_required_adapter() -> AosResult<()> {
    let (_dir, registry) = setup_registry();
    let resolver = registry.dependency_resolver();

    let manifests = BTreeMap::from([(
        "root".to_string(),
        make_deps(Some("qwen"), &["missing"], &[]),
    )]);

    let err = resolver
        .resolve("root", manifests.get("root").unwrap(), "qwen", |id| {
            Ok(manifests.get(id).cloned())
        })
        .unwrap_err();

    match err {
        AosError::Registry(msg) => {
            assert!(msg.contains("requires missing adapter"));
        }
        other => panic!("unexpected error: {other:?}"),
    }

    Ok(())
}

#[test]
fn rejects_circular_dependency() -> AosResult<()> {
    let (_dir, registry) = setup_registry();
    register(&registry, "adapter_a");
    register(&registry, "adapter_b");

    let resolver = registry.dependency_resolver();

    let manifests = BTreeMap::from([
        (
            "adapter_a".to_string(),
            make_deps(Some("qwen"), &["adapter_b"], &[]),
        ),
        (
            "adapter_b".to_string(),
            make_deps(Some("qwen"), &["adapter_a"], &[]),
        ),
    ]);

    let err = resolver
        .resolve(
            "adapter_a",
            manifests.get("adapter_a").unwrap(),
            "qwen",
            |id| Ok(manifests.get(id).cloned()),
        )
        .unwrap_err();

    match err {
        AosError::Registry(msg) => assert!(msg.contains("Circular dependency")),
        other => panic!("unexpected error: {other:?}"),
    }

    Ok(())
}

#[test]
fn rejects_conflicting_dependency_in_graph() -> AosResult<()> {
    let (_dir, registry) = setup_registry();
    register(&registry, "adapter_a");
    register(&registry, "adapter_b");
    register(&registry, "adapter_c");

    let resolver = registry.dependency_resolver();

    let manifests = BTreeMap::from([
        (
            "adapter_a".to_string(),
            make_deps(Some("qwen"), &["adapter_b", "adapter_c"], &[]),
        ),
        (
            "adapter_b".to_string(),
            make_deps(Some("qwen"), &[], &["adapter_c"]),
        ),
        ("adapter_c".to_string(), make_deps(Some("qwen"), &[], &[])),
    ]);

    let err = resolver
        .resolve(
            "adapter_a",
            manifests.get("adapter_a").unwrap(),
            "qwen",
            |id| Ok(manifests.get(id).cloned()),
        )
        .unwrap_err();

    match err {
        AosError::Registry(msg) => {
            assert!(msg.contains("conflict"), "unexpected message: {msg}");
            assert!(
                msg.contains("adapter_b"),
                "missing adapter_b in message: {msg}"
            );
            assert!(
                msg.contains("adapter_c"),
                "missing adapter_c in message: {msg}"
            );
        }
        other => panic!("unexpected error: {other:?}"),
    }

    Ok(())
}

#[test]
fn rejects_conflict_with_registered_adapter() -> AosResult<()> {
    let (_dir, registry) = setup_registry();
    register(&registry, "adapter_b");
    register(&registry, "legacy_adapter_v1");

    let resolver = registry.dependency_resolver();

    let manifests = BTreeMap::from([(
        "adapter_a".to_string(),
        make_deps(Some("qwen"), &["adapter_b"], &["legacy_adapter_v1"]),
    )]);

    let err = resolver
        .resolve(
            "adapter_a",
            manifests.get("adapter_a").unwrap(),
            "qwen",
            |id| Ok(manifests.get(id).cloned()),
        )
        .unwrap_err();

    match err {
        AosError::Registry(msg) => {
            assert!(msg.contains("conflicts with registered adapter"));
        }
        other => panic!("unexpected error: {other:?}"),
    }

    Ok(())
}

#[test]
fn rejects_base_model_mismatch() -> AosResult<()> {
    let (_dir, registry) = setup_registry();
    register(&registry, "adapter_b");

    let resolver = registry.dependency_resolver();

    let manifests = BTreeMap::from([
        (
            "adapter_a".to_string(),
            make_deps(Some("qwen2.5-7b"), &["adapter_b"], &[]),
        ),
        (
            "adapter_b".to_string(),
            make_deps(Some("qwen2.5-7b"), &[], &[]),
        ),
    ]);

    let err = resolver
        .resolve(
            "adapter_a",
            manifests.get("adapter_a").unwrap(),
            "other-base",
            |id| Ok(manifests.get(id).cloned()),
        )
        .unwrap_err();

    match err {
        AosError::Registry(msg) => {
            assert!(msg.contains("requires base model"));
        }
        other => panic!("unexpected error: {other:?}"),
    }

    Ok(())
}

#[test]
fn rejects_missing_manifest_for_dependency() -> AosResult<()> {
    let (_dir, registry) = setup_registry();
    register(&registry, "adapter_b");

    let resolver = registry.dependency_resolver();

    let manifests = BTreeMap::from([(
        "adapter_a".to_string(),
        make_deps(Some("qwen"), &["adapter_b"], &[]),
    )]);

    let err = resolver
        .resolve(
            "adapter_a",
            manifests.get("adapter_a").unwrap(),
            "qwen",
            |id| Ok(manifests.get(id).cloned()),
        )
        .unwrap_err();

    match err {
        AosError::Registry(msg) => {
            assert!(msg.contains("Missing dependency manifest"));
        }
        other => panic!("unexpected error: {other:?}"),
    }

    Ok(())
}
