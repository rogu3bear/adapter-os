//! Document Tenant Isolation Tests
//!
//! These tests verify that the database-level tenant isolation functions
//! correctly enforce tenant boundaries. All `get_*` functions that access
//! tenant-scoped resources must:
//!
//! 1. Accept a `tenant_id` parameter
//! 2. Filter results by `tenant_id` in the WHERE clause
//! 3. Return None/empty for resources belonging to other tenants
//!
//! This is a defense-in-depth measure complementing the handler-level
//! tenant validation and FK constraint enforcement.

use adapteros_core::{AosError, Result};
use adapteros_db::collections::CreateCollectionParams;
use adapteros_db::documents::CreateDocumentParams;
use adapteros_db::Db;
use uuid::Uuid;

/// Create a test tenant
async fn create_test_tenant(db: &Db, tenant_id: &str) -> Result<()> {
    sqlx::query("INSERT INTO tenants (id, name, itar_flag) VALUES (?, ?, 0)")
        .bind(tenant_id)
        .bind(tenant_id)
        .execute(db.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to create tenant: {}", e)))?;
    Ok(())
}

// =============================================================================
// TEST: get_document() tenant isolation
// =============================================================================

#[tokio::test]
async fn test_get_document_enforces_tenant_isolation() -> Result<()> {
    let db = Db::new_in_memory().await?;

    // Create two tenants
    create_test_tenant(&db, "tenant-a").await?;
    create_test_tenant(&db, "tenant-b").await?;

    // Create a document in tenant-a
    let doc_id = Uuid::new_v4().to_string();
    db.create_document(CreateDocumentParams {
        id: doc_id.clone(),
        tenant_id: "tenant-a".to_string(),
        name: "secret-doc.pdf".to_string(),
        content_hash: "hash123".to_string(),
        file_path: "var/test-documents/secret.pdf".to_string(),
        file_size: 1000,
        mime_type: "application/pdf".to_string(),
        page_count: Some(10),
    })
    .await?;

    // Tenant-a can access their own document
    let doc_by_owner = db.get_document("tenant-a", &doc_id).await?;
    assert!(
        doc_by_owner.is_some(),
        "Owner tenant should be able to access their document"
    );
    assert_eq!(doc_by_owner.unwrap().name, "secret-doc.pdf");

    // Tenant-b CANNOT access tenant-a's document
    let doc_by_other = db.get_document("tenant-b", &doc_id).await?;
    assert!(
        doc_by_other.is_none(),
        "Other tenant should NOT be able to access document via get_document"
    );

    // Non-existent tenant also returns None
    let doc_by_fake = db.get_document("fake-tenant", &doc_id).await?;
    assert!(
        doc_by_fake.is_none(),
        "Fake tenant should NOT be able to access document"
    );

    Ok(())
}

// =============================================================================
// TEST: get_documents_by_ids_ordered() tenant isolation
// =============================================================================

#[tokio::test]
async fn test_get_documents_by_ids_ordered_enforces_tenant_isolation() -> Result<()> {
    let db = Db::new_in_memory().await?;

    create_test_tenant(&db, "tenant-a").await?;
    create_test_tenant(&db, "tenant-b").await?;

    // Create documents in tenant-a
    let doc1_id = Uuid::new_v4().to_string();
    db.create_document(CreateDocumentParams {
        id: doc1_id.clone(),
        tenant_id: "tenant-a".to_string(),
        name: "doc1.pdf".to_string(),
        content_hash: "hash1".to_string(),
        file_path: "var/test-documents/doc1.pdf".to_string(),
        file_size: 1000,
        mime_type: "application/pdf".to_string(),
        page_count: None,
    })
    .await?;

    let doc2_id = Uuid::new_v4().to_string();
    db.create_document(CreateDocumentParams {
        id: doc2_id.clone(),
        tenant_id: "tenant-a".to_string(),
        name: "doc2.pdf".to_string(),
        content_hash: "hash2".to_string(),
        file_path: "var/test-documents/doc2.pdf".to_string(),
        file_size: 2000,
        mime_type: "application/pdf".to_string(),
        page_count: None,
    })
    .await?;

    let doc_ids = vec![doc1_id.clone(), doc2_id.clone()];

    // Tenant-a can access their documents
    let docs_by_owner = db
        .get_documents_by_ids_ordered("tenant-a", &doc_ids)
        .await?;
    assert_eq!(docs_by_owner.len(), 2);
    assert!(docs_by_owner[0].is_some());
    assert!(docs_by_owner[1].is_some());

    // Tenant-b gets None for each document
    let docs_by_other = db
        .get_documents_by_ids_ordered("tenant-b", &doc_ids)
        .await?;
    assert_eq!(docs_by_other.len(), 2);
    assert!(
        docs_by_other[0].is_none(),
        "Other tenant should NOT see doc1"
    );
    assert!(
        docs_by_other[1].is_none(),
        "Other tenant should NOT see doc2"
    );

    Ok(())
}

// =============================================================================
// TEST: get_collection() tenant isolation
// =============================================================================

#[tokio::test]
async fn test_get_collection_enforces_tenant_isolation() -> Result<()> {
    let db = Db::new_in_memory().await?;

    create_test_tenant(&db, "tenant-a").await?;
    create_test_tenant(&db, "tenant-b").await?;

    // Create a collection in tenant-a
    let coll_id = db
        .create_collection(CreateCollectionParams {
            tenant_id: "tenant-a".to_string(),
            name: "Secret Collection".to_string(),
            description: Some("Contains sensitive docs".to_string()),
            metadata_json: None,
        })
        .await?;

    // Tenant-a can access their collection
    let coll_by_owner = db.get_collection("tenant-a", &coll_id).await?;
    assert!(
        coll_by_owner.is_some(),
        "Owner tenant should be able to access their collection"
    );
    assert_eq!(coll_by_owner.unwrap().name, "Secret Collection");

    // Tenant-b CANNOT access tenant-a's collection
    let coll_by_other = db.get_collection("tenant-b", &coll_id).await?;
    assert!(
        coll_by_other.is_none(),
        "Other tenant should NOT be able to access collection via get_collection"
    );

    Ok(())
}

// =============================================================================
// TEST: get_document_chunks() tenant isolation
// =============================================================================

#[tokio::test]
async fn test_get_document_chunks_enforces_tenant_isolation() -> Result<()> {
    let db = Db::new_in_memory().await?;

    create_test_tenant(&db, "tenant-a").await?;
    create_test_tenant(&db, "tenant-b").await?;

    // Create a document in tenant-a
    let doc_id = Uuid::new_v4().to_string();
    db.create_document(CreateDocumentParams {
        id: doc_id.clone(),
        tenant_id: "tenant-a".to_string(),
        name: "chunked-doc.pdf".to_string(),
        content_hash: "hash-chunked".to_string(),
        file_path: "var/test-documents/chunked.pdf".to_string(),
        file_size: 5000,
        mime_type: "application/pdf".to_string(),
        page_count: Some(5),
    })
    .await?;

    // Create some chunks for this document
    let chunk1_id = Uuid::new_v4().to_string();
    let chunk2_id = Uuid::new_v4().to_string();

    sqlx::query(
        "INSERT INTO document_chunks (id, tenant_id, document_id, chunk_index, chunk_hash, text_preview)
         VALUES (?, ?, ?, 0, 'chunk-hash-0', 'Preview of chunk 0')",
    )
    .bind(&chunk1_id)
    .bind("tenant-a")
    .bind(&doc_id)
    .execute(db.pool())
    .await?;

    sqlx::query(
        "INSERT INTO document_chunks (id, tenant_id, document_id, chunk_index, chunk_hash, text_preview)
         VALUES (?, ?, ?, 1, 'chunk-hash-1', 'Preview of chunk 1')",
    )
    .bind(&chunk2_id)
    .bind("tenant-a")
    .bind(&doc_id)
    .execute(db.pool())
    .await?;

    // Tenant-a can access their chunks
    let chunks_by_owner = db.get_document_chunks("tenant-a", &doc_id).await?;
    assert_eq!(chunks_by_owner.len(), 2, "Owner should see their 2 chunks");

    // Tenant-b CANNOT access tenant-a's chunks
    let chunks_by_other = db.get_document_chunks("tenant-b", &doc_id).await?;
    assert!(
        chunks_by_other.is_empty(),
        "Other tenant should NOT see any chunks"
    );

    Ok(())
}

// =============================================================================
// TEST: get_collection_documents() tenant isolation
// =============================================================================

#[tokio::test]
async fn test_get_collection_documents_enforces_tenant_isolation() -> Result<()> {
    let db = Db::new_in_memory().await?;

    create_test_tenant(&db, "tenant-a").await?;
    create_test_tenant(&db, "tenant-b").await?;

    // Create a collection and document in tenant-a
    let coll_id = db
        .create_collection(CreateCollectionParams {
            tenant_id: "tenant-a".to_string(),
            name: "Private Collection".to_string(),
            description: None,
            metadata_json: None,
        })
        .await?;

    let doc_id = Uuid::new_v4().to_string();
    db.create_document(CreateDocumentParams {
        id: doc_id.clone(),
        tenant_id: "tenant-a".to_string(),
        name: "private-doc.pdf".to_string(),
        content_hash: "hash-private".to_string(),
        file_path: "var/test-documents/private.pdf".to_string(),
        file_size: 1000,
        mime_type: "application/pdf".to_string(),
        page_count: None,
    })
    .await?;

    // Add document to collection
    db.add_document_to_collection("tenant-a", &coll_id, &doc_id)
        .await?;

    // Tenant-a can see the document in the collection
    let docs_by_owner = db.get_collection_documents("tenant-a", &coll_id).await?;
    assert_eq!(
        docs_by_owner.len(),
        1,
        "Owner should see their document in collection"
    );
    assert_eq!(docs_by_owner[0].name, "private-doc.pdf");

    // Tenant-b CANNOT see documents in tenant-a's collection
    let docs_by_other = db.get_collection_documents("tenant-b", &coll_id).await?;
    assert!(
        docs_by_other.is_empty(),
        "Other tenant should NOT see documents in collection"
    );

    Ok(())
}

// =============================================================================
// TEST: get_document_collections() tenant isolation
// =============================================================================

#[tokio::test]
async fn test_get_document_collections_enforces_tenant_isolation() -> Result<()> {
    let db = Db::new_in_memory().await?;

    create_test_tenant(&db, "tenant-a").await?;
    create_test_tenant(&db, "tenant-b").await?;

    // Create document and collection in tenant-a
    let doc_id = Uuid::new_v4().to_string();
    db.create_document(CreateDocumentParams {
        id: doc_id.clone(),
        tenant_id: "tenant-a".to_string(),
        name: "doc-in-collections.pdf".to_string(),
        content_hash: "hash-coll".to_string(),
        file_path: "var/test-documents/coll.pdf".to_string(),
        file_size: 1000,
        mime_type: "application/pdf".to_string(),
        page_count: None,
    })
    .await?;

    let coll_id = db
        .create_collection(CreateCollectionParams {
            tenant_id: "tenant-a".to_string(),
            name: "Collection Containing Doc".to_string(),
            description: None,
            metadata_json: None,
        })
        .await?;

    db.add_document_to_collection("tenant-a", &coll_id, &doc_id)
        .await?;

    // Tenant-a can see which collections contain their document
    let colls_by_owner = db.get_document_collections("tenant-a", &doc_id).await?;
    assert_eq!(
        colls_by_owner.len(),
        1,
        "Owner should see the collection containing their doc"
    );
    assert_eq!(colls_by_owner[0].name, "Collection Containing Doc");

    // Tenant-b CANNOT see tenant-a's collections
    let colls_by_other = db.get_document_collections("tenant-b", &doc_id).await?;
    assert!(
        colls_by_other.is_empty(),
        "Other tenant should NOT see collections"
    );

    Ok(())
}

// =============================================================================
// TEST: Mixed tenant scenario - partial access
// =============================================================================

#[tokio::test]
async fn test_get_documents_by_ids_mixed_tenants() -> Result<()> {
    let db = Db::new_in_memory().await?;

    create_test_tenant(&db, "tenant-a").await?;
    create_test_tenant(&db, "tenant-b").await?;

    // Create one document in each tenant
    let doc_a = Uuid::new_v4().to_string();
    db.create_document(CreateDocumentParams {
        id: doc_a.clone(),
        tenant_id: "tenant-a".to_string(),
        name: "doc-a.pdf".to_string(),
        content_hash: "hash-a".to_string(),
        file_path: "var/test-documents/a.pdf".to_string(),
        file_size: 1000,
        mime_type: "application/pdf".to_string(),
        page_count: None,
    })
    .await?;

    let doc_b = Uuid::new_v4().to_string();
    db.create_document(CreateDocumentParams {
        id: doc_b.clone(),
        tenant_id: "tenant-b".to_string(),
        name: "doc-b.pdf".to_string(),
        content_hash: "hash-b".to_string(),
        file_path: "var/test-documents/b.pdf".to_string(),
        file_size: 1000,
        mime_type: "application/pdf".to_string(),
        page_count: None,
    })
    .await?;

    // Request both document IDs as tenant-a
    let doc_ids = vec![doc_a.clone(), doc_b.clone()];
    let results = db
        .get_documents_by_ids_ordered("tenant-a", &doc_ids)
        .await?;

    assert_eq!(results.len(), 2);
    // Should see their own document
    assert!(
        results[0].is_some(),
        "tenant-a should see their own document"
    );
    assert_eq!(results[0].as_ref().unwrap().name, "doc-a.pdf");
    // Should NOT see tenant-b's document
    assert!(
        results[1].is_none(),
        "tenant-a should NOT see tenant-b's document"
    );

    Ok(())
}
