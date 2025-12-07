use adapteros_core::Result;
use adapteros_db::kv_backend::KvDb;
use adapteros_db::plans_kv::{PlanKv, PlanKvRepository};
use adapteros_db::repositories_kv::{RepositoryKv, RepositoryKvRepository};
use chrono::{DateTime, Utc};

#[tokio::test]
async fn plans_are_tenant_scoped_and_deterministically_ordered() -> Result<()> {
    let kv = KvDb::init_in_memory()?;
    let repo = PlanKvRepository::new(kv.backend().clone());

    let t1 = "tenant-a";
    let t2 = "tenant-b";
    let ts_old = DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let ts_new = DateTime::parse_from_rfc3339("2024-01-02T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);

    // Two newest share timestamp; tie-breaker uses id ASC.
    repo.put_plan(PlanKv {
        id: "p-02a".into(),
        tenant_id: t1.into(),
        plan_id_b3: "b3:p02a".into(),
        manifest_hash_b3: "mh:p02a".into(),
        kernel_hashes_json: "[]".into(),
        metallib_hash_b3: None,
        created_at: ts_new,
    })
    .await?;
    repo.put_plan(PlanKv {
        id: "p-02b".into(),
        tenant_id: t1.into(),
        plan_id_b3: "b3:p02b".into(),
        manifest_hash_b3: "mh:p02b".into(),
        kernel_hashes_json: "[]".into(),
        metallib_hash_b3: None,
        created_at: ts_new,
    })
    .await?;
    repo.put_plan(PlanKv {
        id: "p-01".into(),
        tenant_id: t1.into(),
        plan_id_b3: "b3:p01".into(),
        manifest_hash_b3: "mh:p01".into(),
        kernel_hashes_json: "[]".into(),
        metallib_hash_b3: None,
        created_at: ts_old,
    })
    .await?;

    // Cross-tenant entry should never leak.
    repo.put_plan(PlanKv {
        id: "p-cross".into(),
        tenant_id: t2.into(),
        plan_id_b3: "b3:pcross".into(),
        manifest_hash_b3: "mh:pcross".into(),
        kernel_hashes_json: "[]".into(),
        metallib_hash_b3: None,
        created_at: ts_new,
    })
    .await?;

    let ordered = repo.list_plans(t1).await?;
    let ids: Vec<_> = ordered.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, vec!["p-02a", "p-02b", "p-01"]);
    assert!(ordered.iter().all(|p| p.tenant_id == t1));

    Ok(())
}

#[tokio::test]
async fn repositories_use_tenant_prefix_and_stable_ordering() -> Result<()> {
    let kv = KvDb::init_in_memory()?;
    let repo = RepositoryKvRepository::new(kv.backend().clone());

    let t1 = "tenant-a";
    let t2 = "tenant-b";

    // Newest timestamp, tie-breaker id DESC.
    repo.put_repository(&RepositoryKv {
        id: "repo-b".into(),
        tenant_id: t1.into(),
        repo_id: "r-b".into(),
        path: "/r/b".into(),
        languages_json: None,
        frameworks_json: None,
        default_branch: "main".into(),
        latest_scan_commit: None,
        latest_scan_at: None,
        latest_graph_hash: None,
        status: "ready".into(),
        created_at: "2024-02-02T00:00:00Z".into(),
        updated_at: "2024-02-02T00:00:00Z".into(),
    })
    .await?;

    repo.put_repository(&RepositoryKv {
        id: "repo-a".into(),
        tenant_id: t1.into(),
        repo_id: "r-a".into(),
        path: "/r/a".into(),
        languages_json: None,
        frameworks_json: None,
        default_branch: "main".into(),
        latest_scan_commit: None,
        latest_scan_at: None,
        latest_graph_hash: None,
        status: "ready".into(),
        created_at: "2024-02-02T00:00:00Z".into(),
        updated_at: "2024-02-02T00:00:00Z".into(),
    })
    .await?;

    repo.put_repository(&RepositoryKv {
        id: "repo-old".into(),
        tenant_id: t1.into(),
        repo_id: "r-old".into(),
        path: "/r/old".into(),
        languages_json: None,
        frameworks_json: None,
        default_branch: "main".into(),
        latest_scan_commit: None,
        latest_scan_at: None,
        latest_graph_hash: None,
        status: "ready".into(),
        created_at: "2024-01-01T00:00:00Z".into(),
        updated_at: "2024-01-01T00:00:00Z".into(),
    })
    .await?;

    // Cross-tenant entry.
    repo.put_repository(&RepositoryKv {
        id: "repo-other".into(),
        tenant_id: t2.into(),
        repo_id: "r-other".into(),
        path: "/r/other".into(),
        languages_json: None,
        frameworks_json: None,
        default_branch: "main".into(),
        latest_scan_commit: None,
        latest_scan_at: None,
        latest_graph_hash: None,
        status: "ready".into(),
        created_at: "2024-03-01T00:00:00Z".into(),
        updated_at: "2024-03-01T00:00:00Z".into(),
    })
    .await?;

    let ordered = repo.list_repositories(t1, 10, 0).await?;
    let ids: Vec<_> = ordered.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids, vec!["repo-b", "repo-a", "repo-old"]);
    assert!(ordered.iter().all(|r| r.tenant_id == t1));

    // Cross-tenant list should not leak t1 entries.
    let other = repo.list_repositories(t2, 10, 0).await?;
    assert!(other.iter().all(|r| r.tenant_id == t2));
    assert!(other.iter().all(|r| r.id != "repo-b"));

    Ok(())
}
