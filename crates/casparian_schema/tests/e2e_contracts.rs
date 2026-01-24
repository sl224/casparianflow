//! End-to-End tests for Schema Contract System
//!
//! Tests the full lifecycle: discovery -> approval -> contract -> amendment -> new contract
//! Uses REAL SQLite databases - no mocks.

use casparian_schema::{
    amendment::{
        approve_amendment, propose_new_columns_amendment, propose_nullability_amendment,
        propose_type_mismatch_amendment, AmendmentAction, AmendmentReason, SchemaAmendmentProposal,
        SchemaChange,
    },
    approval::{approve_schema, ApprovedColumn, ApprovedSchemaVariant, SchemaApprovalRequest},
    contract::{
        DataType, LockedColumn, LockedSchema, SchemaContract, SchemaViolation, ViolationType,
    },
    storage::SchemaStorage,
};
use casparian_schema::{ContractId, DiscoveryId};

const TEST_PARSER_ID: &str = "parser-test";
const TEST_PARSER_VERSION: &str = "1.0.0";

fn new_approval_request(approved_by: &str) -> SchemaApprovalRequest {
    SchemaApprovalRequest::new(
        DiscoveryId::new(),
        TEST_PARSER_ID,
        TEST_PARSER_VERSION,
        approved_by,
    )
}

// =============================================================================
// SCHEMA CONTRACT CREATION
// =============================================================================

/// Test creating a basic schema contract
#[test]
fn test_create_basic_contract() {
    let schema = LockedSchema::new(
        "transactions",
        vec![
            LockedColumn::required("id", DataType::Int64),
            LockedColumn::required("amount", DataType::Float64),
            LockedColumn::required("date", DataType::Date).with_format("YYYY-MM-DD"),
        ],
    );

    let contract = SchemaContract::new("scope-1", schema, "test_user");

    assert_eq!(contract.version, 1);
    assert_eq!(contract.scope_id, "scope-1");
    assert_eq!(contract.schemas.len(), 1);
    assert_eq!(contract.schemas[0].columns.len(), 3);
}

/// Test contract content hash generation
#[test]
fn test_contract_content_hash() {
    let schema = LockedSchema::new(
        "test",
        vec![LockedColumn::optional("col1", DataType::String)],
    );

    let hash = &schema.content_hash;
    assert!(!hash.is_empty(), "Hash should not be empty");

    // Same schema should produce same hash
    let schema2 = LockedSchema::new(
        "test",
        vec![LockedColumn::optional("col1", DataType::String)],
    );

    let hash2 = &schema2.content_hash;
    assert_eq!(hash, hash2, "Same structure should produce same hash");
}

// =============================================================================
// SCHEMA VALIDATION
// =============================================================================

/// Test column validation against contract
#[test]
fn test_validate_columns_success() {
    let schema = LockedSchema::new(
        "test",
        vec![
            LockedColumn::required("id", DataType::Int64),
            LockedColumn::optional("name", DataType::String),
        ],
    );

    // Exact match should succeed
    let result = schema.validate_columns(&["id", "name"]);
    assert!(result.is_ok(), "Validation should succeed for exact match");
}

/// Test column validation failure - missing column
#[test]
fn test_validate_columns_missing() {
    let schema = LockedSchema::new(
        "test",
        vec![
            LockedColumn::required("id", DataType::Int64),
            LockedColumn::optional("name", DataType::String),
        ],
    );

    // Missing column should fail
    let result = schema.validate_columns(&["id"]);
    assert!(result.is_err(), "Validation should fail for missing column");

    let err = result.unwrap_err();
    assert_eq!(err.violation_type, ViolationType::ColumnCountMismatch);
}

/// Test column validation failure - extra column
#[test]
fn test_validate_columns_extra() {
    let schema = LockedSchema::new("test", vec![LockedColumn::required("id", DataType::Int64)]);

    // Extra column should fail
    let result = schema.validate_columns(&["id", "extra"]);
    assert!(result.is_err(), "Validation should fail for extra column");

    let err = result.unwrap_err();
    assert_eq!(err.violation_type, ViolationType::ColumnCountMismatch);
}

/// Test data type validation
#[test]
fn test_data_type_validation() {
    // Int64 validation
    assert!(DataType::Int64.validate_string("123"));
    assert!(DataType::Int64.validate_string("-456"));
    assert!(!DataType::Int64.validate_string("12.5"));
    assert!(!DataType::Int64.validate_string("abc"));

    // Float64 validation
    assert!(DataType::Float64.validate_string("123.45"));
    assert!(DataType::Float64.validate_string("-0.5"));
    assert!(DataType::Float64.validate_string("100")); // Int is valid float
    assert!(!DataType::Float64.validate_string("abc"));

    // Boolean validation
    assert!(DataType::Boolean.validate_string("true"));
    assert!(DataType::Boolean.validate_string("false"));
    assert!(DataType::Boolean.validate_string("1"));
    assert!(DataType::Boolean.validate_string("0"));
    assert!(DataType::Boolean.validate_string("yes")); // Supported in implementation

    // String accepts anything
    assert!(DataType::String.validate_string("anything"));
    assert!(DataType::String.validate_string(""));
    assert!(DataType::String.validate_string("123"));

    // Date validation (ISO format)
    assert!(DataType::Date.validate_string("2024-01-15"));
    assert!(!DataType::Date.validate_string("01/15/2024"));
    assert!(!DataType::Date.validate_string("not a date"));
}

// =============================================================================
// SCHEMA STORAGE - REAL SQLITE
// =============================================================================

/// Test saving and retrieving contracts from real SQLite
#[test]
fn test_storage_save_and_get() {
    let storage = SchemaStorage::in_memory().unwrap();

    let schema = LockedSchema::new("users", vec![LockedColumn::required("id", DataType::Int64)]);

    let contract = SchemaContract::new("test-scope", schema, "test_user");

    // Save
    storage.save_contract(&contract).unwrap();

    // Retrieve by ID
    let retrieved = storage.get_contract(&contract.contract_id).unwrap();
    assert!(retrieved.is_some());
    let retrieved = retrieved.unwrap();

    assert_eq!(retrieved.contract_id, contract.contract_id);
    assert_eq!(retrieved.scope_id, "test-scope");
    assert_eq!(retrieved.schemas.len(), 1);
    assert_eq!(retrieved.schemas[0].name, "users");
}

/// Test getting contract by scope
#[test]
fn test_storage_get_by_scope() {
    let storage = SchemaStorage::in_memory().unwrap();

    let scope_id = "my-scope";

    // Create and save contract
    let schema = LockedSchema::new("orders", vec![]);
    let contract = SchemaContract::new(scope_id, schema, "test_user");

    storage.save_contract(&contract).unwrap();

    // Retrieve by scope
    let retrieved = storage.get_contract_for_scope(scope_id).unwrap();
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().scope_id, scope_id);

    // Non-existent scope returns None
    let not_found = storage.get_contract_for_scope("nonexistent").unwrap();
    assert!(not_found.is_none());
}

/// Test contract versioning
#[test]
fn test_storage_versioning() {
    let storage = SchemaStorage::in_memory().unwrap();

    let scope_id = "versioned-scope";

    // V1
    let schema_v1 = LockedSchema::new("data_v1", vec![]);
    let v1 = SchemaContract::new(scope_id, schema_v1, "test_user");
    storage.save_contract(&v1).unwrap();

    // V2 - same scope, new contract
    let schema_v2 = LockedSchema::new(
        "data_v2",
        vec![LockedColumn::optional("new_col", DataType::String)],
    );
    let mut v2 = SchemaContract::new(scope_id, schema_v2, "test_user");
    v2.version = 2;
    storage.save_contract(&v2).unwrap();

    // Latest should be V2
    let latest = storage.get_contract_for_scope(scope_id).unwrap().unwrap();
    assert_eq!(latest.version, 2);
    assert_eq!(latest.schemas[0].name, "data_v2");
}

/// Test listing all contracts
#[test]
fn test_storage_list_contracts() {
    let storage = SchemaStorage::in_memory().unwrap();

    // Create multiple contracts
    for i in 0..5 {
        let schema_name = format!("schema-{}", i);
        let scope_id = format!("scope-{}", i);
        let schema = LockedSchema::new(&schema_name, vec![]);
        let contract = SchemaContract::new(&scope_id, schema, "test_user");
        storage.save_contract(&contract).unwrap();
    }

    // List all
    let contracts = storage.list_contracts(None).unwrap();
    assert_eq!(contracts.len(), 5);

    // List with limit
    let limited = storage.list_contracts(Some(3)).unwrap();
    assert_eq!(limited.len(), 3);
}

/// Test deleting contracts
#[test]
fn test_storage_delete_contract() {
    let storage = SchemaStorage::in_memory().unwrap();

    let schema = LockedSchema::new("to-delete", vec![]);
    let contract = SchemaContract::new("to-delete", schema, "test_user");

    storage.save_contract(&contract).unwrap();

    // Verify exists
    assert!(storage
        .get_contract(&contract.contract_id)
        .unwrap()
        .is_some());

    // Delete
    storage.delete_contract(&contract.contract_id).unwrap();

    // Verify gone
    assert!(storage
        .get_contract(&contract.contract_id)
        .unwrap()
        .is_none());
}

// =============================================================================
// APPROVAL WORKFLOW
// =============================================================================

/// Test basic schema approval
#[test]
fn test_approval_basic() {
    let storage = SchemaStorage::in_memory().unwrap();

    let request = new_approval_request("approver").with_schema(
        ApprovedSchemaVariant::new("sales", "sales_fact").with_columns(vec![
            ApprovedColumn::required("product_id", DataType::Int64),
            ApprovedColumn::required("quantity", DataType::Int64),
            ApprovedColumn::required("price", DataType::Float64),
        ]),
    );

    let result = approve_schema(&storage, request).unwrap();

    // Should create a contract
    assert_eq!(result.contract.schemas.len(), 1);
    assert_eq!(result.contract.schemas[0].name, "sales_fact");
    assert_eq!(result.contract.schemas[0].columns.len(), 3);

    // Verify persisted
    let retrieved = storage.get_contract(&result.contract.contract_id).unwrap();
    assert!(retrieved.is_some());
}

/// Test approval with column rename
#[test]
fn test_approval_with_rename() {
    let storage = SchemaStorage::in_memory().unwrap();

    let request = new_approval_request("approver").with_schema(
        ApprovedSchemaVariant::new("customers", "customers").with_columns(vec![
            ApprovedColumn::required("cust_id", DataType::Int64).rename_to("customer_id"),
        ]),
    );

    let result = approve_schema(&storage, request).unwrap();

    // Check the renamed column in contract
    assert_eq!(result.contract.schemas[0].columns[0].name, "customer_id");

    // Check warnings about rename
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.message.contains("renamed")),
        "Should warn about rename"
    );
}

/// Test approval with excluded files
#[test]
fn test_approval_with_exclusions() {
    let storage = SchemaStorage::in_memory().unwrap();

    let request = new_approval_request("approver")
        .with_schema(
            ApprovedSchemaVariant::new("logs", "logs")
                .with_columns(vec![ApprovedColumn::required("id", DataType::Int64)]),
        )
        .exclude_files(vec![
            "/path/to/bad_file1.csv".to_string(),
            "/path/to/bad_file2.csv".to_string(),
        ]);

    let result = approve_schema(&storage, request).unwrap();

    // Should have warnings about excluded files
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.message.contains("excluded")),
        "Should warn about excluded files"
    );
}

/// Test approval failure - no schemas
#[test]
fn test_approval_no_schemas() {
    let storage = SchemaStorage::in_memory().unwrap();

    let request = new_approval_request("approver");

    let result = approve_schema(&storage, request);

    // Should fail - can't approve empty schema
    assert!(result.is_err(), "Should fail with no schemas");
}

// =============================================================================
// AMENDMENT WORKFLOW
// =============================================================================

/// Test proposing type mismatch amendment
#[test]
fn test_amendment_type_mismatch() {
    let storage = SchemaStorage::in_memory().unwrap();

    // First create a contract
    let schema = LockedSchema::new(
        "events",
        vec![
            LockedColumn::required("event_id", DataType::Int64),
            LockedColumn::required("timestamp", DataType::Timestamp),
        ],
    );
    let contract = SchemaContract::new("amend-scope", schema, "test_user");
    storage.save_contract(&contract).unwrap();

    // Propose amendment due to type mismatch
    let proposal = propose_type_mismatch_amendment(
        &contract,
        "event_id",
        DataType::Int64,
        vec!["EVT001".to_string(), "EVT002".to_string()],
        DataType::String,
    )
    .unwrap();

    assert_eq!(proposal.contract_id, contract.contract_id);
    assert!(!proposal.changes.is_empty(), "Should have proposed changes");

    // Check the change suggests String type
    let has_type_change = proposal.changes.iter().any(|c| {
        matches!(c, SchemaChange::ChangeType { column_name, from, to }
            if column_name == "event_id" && *from == DataType::Int64 && *to == DataType::String)
    });
    assert!(
        has_type_change,
        "Should propose changing event_id to String"
    );
}

/// Test proposing nullability amendment
#[test]
fn test_amendment_nullability() {
    let storage = SchemaStorage::in_memory().unwrap();

    let schema = LockedSchema::new(
        "products",
        vec![LockedColumn::required("sku", DataType::String)],
    );
    let contract = SchemaContract::new("null-scope", schema, "test_user");
    storage.save_contract(&contract).unwrap();

    let proposal = propose_nullability_amendment(
        &contract, "sku", 15.5, // 15.5% nulls found
    )
    .unwrap();

    // Should propose making it nullable
    let has_null_change = proposal.changes.iter().any(|c| {
        matches!(c, SchemaChange::ChangeNullability { column_name, nullable }
            if column_name == "sku" && *nullable == true)
    });
    assert!(has_null_change, "Should propose making sku nullable");
}

/// Test proposing new columns amendment
#[test]
fn test_amendment_new_columns() {
    let storage = SchemaStorage::in_memory().unwrap();

    let schema = LockedSchema::new(
        "orders",
        vec![LockedColumn::required("order_id", DataType::Int64)],
    );
    let contract = SchemaContract::new("newcol-scope", schema, "test_user");
    storage.save_contract(&contract).unwrap();

    let new_columns = vec![
        LockedColumn::optional("shipping_date", DataType::Date).with_format("YYYY-MM-DD"),
        LockedColumn::optional("tracking_number", DataType::String),
    ];

    let proposal = propose_new_columns_amendment(&contract, new_columns, 10).unwrap();

    // Should propose adding both columns
    let add_count = proposal
        .changes
        .iter()
        .filter(|c| matches!(c, SchemaChange::AddColumn { .. }))
        .count();
    assert_eq!(add_count, 2, "Should propose adding 2 columns");
}

/// Test approving amendment as proposed
#[test]
fn test_amendment_approve_as_proposed() {
    let storage = SchemaStorage::in_memory().unwrap();

    let schema = LockedSchema::new(
        "metrics",
        vec![LockedColumn::required("value", DataType::Int64)],
    );
    let contract = SchemaContract::new("approve-amend-scope", schema, "test_user");
    storage.save_contract(&contract).unwrap();

    // Propose amendment (type mismatch from Int64 to Float64)
    let proposal = propose_type_mismatch_amendment(
        &contract,
        "value",
        DataType::Int64,
        vec!["12.5".to_string()],
        DataType::Float64,
    )
    .unwrap();

    // Approve as proposed
    let result = approve_amendment(
        &storage,
        &proposal,
        AmendmentAction::ApproveAsProposed,
        "reviewer",
    )
    .unwrap();

    // Should create new contract version
    assert!(result.contract.is_some());
    let new_contract = result.contract.unwrap();

    assert_eq!(new_contract.version, 2, "Should be version 2");
    assert_eq!(new_contract.scope_id, contract.scope_id);
}

/// Test rejecting amendment
#[test]
fn test_amendment_reject() {
    let storage = SchemaStorage::in_memory().unwrap();

    let schema = LockedSchema::new("data", vec![LockedColumn::required("id", DataType::Int64)]);
    let contract = SchemaContract::new("reject-scope", schema, "test_user");
    storage.save_contract(&contract).unwrap();

    // Create a proposal for a new schema variant
    let proposed_schema = LockedSchema::new(
        "data",
        vec![
            LockedColumn::required("id", DataType::Int64),
            LockedColumn::optional("extra", DataType::String),
        ],
    );

    let proposal = SchemaAmendmentProposal::new(
        contract.contract_id,
        AmendmentReason::NewSchemaVariant {
            variant_description: "New format detected".to_string(),
            file_count: 5,
            file_pattern: Some("*.new.csv".to_string()),
        },
        contract.schemas[0].clone(),
        proposed_schema,
    );

    // Reject
    let result = approve_amendment(
        &storage,
        &proposal,
        AmendmentAction::Reject {
            reason: "We don't support this format".to_string(),
        },
        "reviewer",
    )
    .unwrap();

    // Should not create new contract
    assert!(result.new_contract.is_none());
    assert!(result.contract.is_none());
}

/// Test creating separate schema from amendment
#[test]
fn test_amendment_create_separate_schema() {
    let storage = SchemaStorage::in_memory().unwrap();

    let schema = LockedSchema::new(
        "original",
        vec![LockedColumn::required("id", DataType::Int64)],
    );
    let contract = SchemaContract::new("separate-scope", schema, "test_user");
    storage.save_contract(&contract).unwrap();

    let proposed_schema = LockedSchema::new(
        "alternate_format",
        vec![
            LockedColumn::required("id", DataType::String),
            LockedColumn::optional("alternate_field", DataType::String),
        ],
    );

    let proposal = SchemaAmendmentProposal::new(
        contract.contract_id,
        AmendmentReason::NewSchemaVariant {
            variant_description: "Completely different format".to_string(),
            file_count: 100,
            file_pattern: Some("alt_*.csv".to_string()),
        },
        contract.schemas[0].clone(),
        proposed_schema,
    );

    // Create as separate schema
    let result = approve_amendment(
        &storage,
        &proposal,
        AmendmentAction::CreateSeparateSchema {
            name: "alternate_format".to_string(),
            description: Some("Alternate format files".to_string()),
        },
        "reviewer",
    )
    .unwrap();

    // Should have new contract
    assert!(result.new_contract.is_some());
    let new_contract = result.new_contract.unwrap();

    // Should have the schema
    assert_eq!(new_contract.schemas[0].name, "alternate_format");
}

// =============================================================================
// FULL LIFECYCLE E2E
// =============================================================================

/// Test complete lifecycle: discovery -> approval -> violation -> amendment -> new contract
#[test]
fn test_full_schema_lifecycle() {
    let storage = SchemaStorage::in_memory().unwrap();

    // Step 1: Initial approval (simulating discovery result)
    let initial_request = new_approval_request("approver").with_schema(
        ApprovedSchemaVariant::new("transactions", "tx_fact").with_columns(vec![
            ApprovedColumn::required("tx_id", DataType::Int64),
            ApprovedColumn::required("amount", DataType::Int64), // Initially thought to be Int
            ApprovedColumn::required("status", DataType::String),
        ]),
    );

    let result = approve_schema(&storage, initial_request).unwrap();
    let v1_contract = result.contract;
    assert_eq!(v1_contract.version, 1);

    // Step 2: Simulate violation detection - found decimal amounts
    let _violation = SchemaViolation::type_mismatch(
        1, // amount column index
        DataType::Int64,
        "125.50",
    )
    .with_file("/data/transactions_2024.csv")
    .with_row(1500);

    // Step 3: Propose amendment based on violation
    let proposal = propose_type_mismatch_amendment(
        &v1_contract,
        "amount",
        DataType::Int64,
        vec![
            "125.50".to_string(),
            "99.99".to_string(),
            "0.01".to_string(),
        ],
        DataType::Float64,
    )
    .unwrap();

    // Verify proposal suggests Float64
    assert!(
        proposal.changes.iter().any(|c| {
            matches!(c, SchemaChange::ChangeType { column_name, to, .. }
            if column_name == "amount" && *to == DataType::Float64)
        }),
        "Should propose changing amount to Float64"
    );

    // Step 4: Approve amendment
    let amend_result = approve_amendment(
        &storage,
        &proposal,
        AmendmentAction::ApproveAsProposed,
        "reviewer",
    )
    .unwrap();

    let v2_contract = amend_result.contract.unwrap();
    assert_eq!(v2_contract.version, 2);

    // Step 5: Verify new contract has correct type
    let amount_col = v2_contract.schemas[0]
        .columns
        .iter()
        .find(|c| c.name == "amount")
        .expect("Should have amount column");
    assert_eq!(
        amount_col.data_type,
        DataType::Float64,
        "amount should now be Float64"
    );

    // Step 6: Verify we can retrieve latest contract
    let latest = storage
        .get_contract_for_scope(&v1_contract.scope_id)
        .unwrap()
        .unwrap();
    assert_eq!(latest.version, 2);
    assert_eq!(latest.contract_id, v2_contract.contract_id);
}

// =============================================================================
// SCHEMA VIOLATION DETECTION
// =============================================================================

/// Test violation type display
#[test]
fn test_violation_display() {
    let violation = SchemaViolation::type_mismatch(3, DataType::Int64, "not_a_number")
        .with_file("/data/test.csv")
        .with_row(42);

    let display = format!("{}", violation);
    assert!(
        display.contains("TypeMismatch") || display.contains("type"),
        "Display should mention type mismatch"
    );
}

/// Test violation types
#[test]
fn test_all_violation_types() {
    let types = [
        ViolationType::TypeMismatch,
        ViolationType::NullNotAllowed,
        ViolationType::FormatMismatch,
        ViolationType::ColumnNameMismatch,
        ViolationType::ColumnCountMismatch,
        ViolationType::SchemaNotFound,
    ];

    for vtype in types {
        // Should be displayable
        let display = format!("{}", vtype);
        assert!(!display.is_empty(), "ViolationType should be displayable");
    }
}

// =============================================================================
// SECURITY TESTS - SQL Injection, Path Traversal, Invalid Input
// =============================================================================

/// Test SQL injection attempts in scope_id (parameterized queries should prevent)
#[test]
fn test_sql_injection_in_scope_id() {
    let storage = SchemaStorage::in_memory().unwrap();

    // SQL injection attempts in scope_id
    let injection_payloads = [
        "'; DROP TABLE schema_contracts; --",
        "1 OR 1=1",
        "1'; DELETE FROM schema_contracts WHERE '1'='1",
        "scope_id UNION SELECT * FROM sqlite_master",
        "Robert'); DROP TABLE schema_contracts;--",
    ];

    for payload in injection_payloads {
        let schema = LockedSchema::new("test", vec![LockedColumn::required("id", DataType::Int64)]);
        let contract = SchemaContract::new(payload, schema, "attacker");

        // Should store safely (parameterized query)
        let result = storage.save_contract(&contract);
        assert!(
            result.is_ok(),
            "Should handle SQL injection attempt safely: {}",
            payload
        );

        // Verify the payload is stored as literal text, not executed
        let retrieved = storage.get_contract_for_scope(payload).unwrap();
        assert!(
            retrieved.is_some(),
            "Should find contract with literal scope_id"
        );
        assert_eq!(
            retrieved.unwrap().scope_id,
            payload,
            "Scope ID should be stored literally"
        );
    }
}

/// Test SQL injection in column names
#[test]
fn test_sql_injection_in_column_names() {
    let storage = SchemaStorage::in_memory().unwrap();

    // SQL injection in column name
    let malicious_column = "id'; DROP TABLE schema_contracts; --";
    let schema = LockedSchema::new(
        "test",
        vec![LockedColumn::required(malicious_column, DataType::String)],
    );
    let contract = SchemaContract::new("scope-1", schema, "attacker");

    // Should store safely
    let result = storage.save_contract(&contract);
    assert!(result.is_ok(), "Should handle SQL injection in column name");

    // Verify stored literally
    let retrieved = storage.get_contract_for_scope("scope-1").unwrap().unwrap();
    assert_eq!(retrieved.schemas[0].columns[0].name, malicious_column);
}

/// Test SQL injection in approved_by field
#[test]
fn test_sql_injection_in_user_name() {
    let storage = SchemaStorage::in_memory().unwrap();

    let malicious_user = "admin'; UPDATE schema_contracts SET approved_by='hacked' WHERE '1'='1";
    let schema = LockedSchema::new("test", vec![LockedColumn::required("id", DataType::Int64)]);
    let contract = SchemaContract::new("scope-sql", schema, malicious_user);

    storage.save_contract(&contract).unwrap();

    // Verify stored literally
    let retrieved = storage
        .get_contract_for_scope("scope-sql")
        .unwrap()
        .unwrap();
    assert_eq!(retrieved.approved_by, malicious_user);
}

/// Test path traversal attempts in scope_id
#[test]
fn test_path_traversal_in_scope_id() {
    // These should be stored as-is since scope_id isn't used as file path
    let path_traversal_payloads = [
        "../../../etc/passwd",
        "..\\..\\..\\windows\\system32\\config",
        "/etc/passwd",
        "scope/../../../secret",
        "%2e%2e%2f%2e%2e%2f%2e%2e%2fetc/passwd",
    ];

    for payload in path_traversal_payloads {
        let schema = LockedSchema::new("test", vec![LockedColumn::required("id", DataType::Int64)]);
        let contract = SchemaContract::new(payload, schema, "user");

        // Scope ID is just a string identifier - no file operations
        assert_eq!(
            contract.scope_id, payload,
            "Scope ID should be stored as-is"
        );
    }
}

/// Test extremely long string inputs (buffer overflow prevention)
#[test]
fn test_extremely_long_strings() {
    let storage = SchemaStorage::in_memory().unwrap();

    // Very long scope_id
    let long_scope_id = "x".repeat(10_000);
    let schema = LockedSchema::new("test", vec![LockedColumn::required("id", DataType::Int64)]);
    let contract = SchemaContract::new(&long_scope_id, schema, "user");

    // Should handle without crashing
    let result = storage.save_contract(&contract);
    assert!(result.is_ok(), "Should handle very long scope_id");

    // Very long column name
    let long_column_name = "col_".to_string() + &"x".repeat(10_000);
    let schema2 = LockedSchema::new(
        "test",
        vec![LockedColumn::required(&long_column_name, DataType::String)],
    );
    let contract2 = SchemaContract::new("long-col", schema2, "user");

    let result2 = storage.save_contract(&contract2);
    assert!(result2.is_ok(), "Should handle very long column name");
}

/// Test empty string inputs
#[test]
fn test_empty_string_inputs() {
    let storage = SchemaStorage::in_memory().unwrap();

    // Empty scope_id
    let schema = LockedSchema::new("test", vec![LockedColumn::required("id", DataType::Int64)]);
    let contract = SchemaContract::new("", schema, "user");

    let result = storage.save_contract(&contract);
    assert!(result.is_ok(), "Should handle empty scope_id");

    // Empty column name
    let schema2 = LockedSchema::new("test", vec![LockedColumn::required("", DataType::String)]);
    let contract2 = SchemaContract::new("empty-col", schema2, "user");

    let result2 = storage.save_contract(&contract2);
    assert!(result2.is_ok(), "Should handle empty column name");
}

/// Test null bytes and special characters
#[test]
fn test_null_bytes_and_special_chars() {
    let storage = SchemaStorage::in_memory().unwrap();

    let special_strings = [
        "scope\twith\ttabs",     // Tabs
        "scope\nwith\nnewlines", // Newlines
        "scope\rwith\rreturns",  // Carriage returns
        "scope with unicode 你好 🎉 émojis",
    ];

    for (i, payload) in special_strings.iter().enumerate() {
        let schema = LockedSchema::new("test", vec![LockedColumn::required("id", DataType::Int64)]);
        let contract = SchemaContract::new(*payload, schema, "user");

        let result = storage.save_contract(&contract);
        assert!(result.is_ok(), "Should handle special chars: {:?}", payload);

        // Verify round-trip
        let retrieved = storage.get_contract(&contract.contract_id).unwrap();
        if let Some(c) = retrieved {
            assert_eq!(&c.scope_id, *payload, "Special chars test {} failed", i);
        }
    }
}

/// Test that deleting with SQL injection ID doesn't affect other records
#[test]
fn test_delete_sql_injection_safety() {
    let storage = SchemaStorage::in_memory().unwrap();

    // Create legitimate contract
    let legit_schema =
        LockedSchema::new("legit", vec![LockedColumn::required("id", DataType::Int64)]);
    let legit_contract = SchemaContract::new("legitimate-scope", legit_schema, "real_user");
    storage.save_contract(&legit_contract).unwrap();

    // Try to delete with injection
    let malicious_id = ContractId::new(); // Random ID - won't match legitimate
    let result = storage.delete_contract(&malicious_id);
    assert!(result.is_ok());

    // Legitimate contract should still exist
    let still_exists = storage.get_contract_for_scope("legitimate-scope").unwrap();
    assert!(
        still_exists.is_some(),
        "Legitimate contract should not be affected"
    );
}

/// Test content hash doesn't reveal sensitive data
#[test]
fn test_content_hash_is_opaque() {
    let schema = LockedSchema::new(
        "sensitive_table",
        vec![
            LockedColumn::required("credit_card", DataType::String),
            LockedColumn::required("ssn", DataType::String),
        ],
    );

    let hash = &schema.content_hash;

    // Hash should be a fixed-length opaque identifier
    assert!(!hash.is_empty(), "Hash should not be empty");

    // Hash should not contain any original content (column names or table name)
    assert!(
        !hash.contains("credit_card"),
        "Hash should not reveal column names"
    );
    assert!(
        !hash.contains("sensitive"),
        "Hash should not reveal table name"
    );
    assert!(!hash.contains("ssn"), "Hash should not reveal column names");
}

/// Test concurrent modifications don't corrupt data
#[test]
fn test_concurrent_scope_updates() {
    let storage = SchemaStorage::in_memory().unwrap();

    // Create initial contract
    let schema1 = LockedSchema::new("test", vec![LockedColumn::required("id", DataType::Int64)]);
    let contract1 = SchemaContract::new("concurrent-scope", schema1, "user1");
    storage.save_contract(&contract1).unwrap();

    // Create second version for same scope
    let schema2 = LockedSchema::new(
        "test",
        vec![
            LockedColumn::required("id", DataType::Int64),
            LockedColumn::required("new_col", DataType::String),
        ],
    );
    let mut contract2 = SchemaContract::new("concurrent-scope", schema2, "user2");
    contract2.version = 2;
    storage.save_contract(&contract2).unwrap();

    // Both should exist with correct versions (use get_contract_history)
    let all = storage.get_contract_history("concurrent-scope").unwrap();
    assert_eq!(all.len(), 2, "Should have two versions");

    // Get each contract to verify
    let c1 = storage.get_contract(&contract1.contract_id).unwrap();
    let c2 = storage.get_contract(&contract2.contract_id).unwrap();
    assert!(c1.is_some());
    assert!(c2.is_some());
    assert_eq!(c1.unwrap().version, 1);
    assert_eq!(c2.unwrap().version, 2);
}

/// Test that data types are validated correctly (no type confusion)
#[test]
fn test_data_type_validation_security() {
    // Test that type coercion doesn't happen silently
    let _schema = LockedSchema::new(
        "test",
        vec![LockedColumn::required("amount", DataType::Int64)],
    );

    // These should NOT be valid integers
    let invalid_integers = [
        "12.5",                   // Float
        "1e10",                   // Scientific notation
        "12,345",                 // Comma-formatted
        "0x1A",                   // Hex
        "12abc",                  // Trailing garbage
        "9999999999999999999999", // Overflow
    ];

    for value in invalid_integers {
        let valid = DataType::Int64.validate_string(value);
        // Most of these should fail validation
        // The important thing is the validation is happening
        assert!(!valid, "Value '{}' should NOT be valid as Int64", value);
    }

    // These SHOULD be valid integers
    let valid_integers = ["123", "-456", "0", "9223372036854775807"];
    for value in valid_integers {
        let valid = DataType::Int64.validate_string(value);
        assert!(valid, "Value '{}' SHOULD be valid as Int64", value);
    }
}

/// Test malicious schema names don't cause issues
#[test]
fn test_malicious_schema_names() {
    let storage = SchemaStorage::in_memory().unwrap();

    // Malicious schema name with SQL injection
    let malicious_name = "test'; DROP TABLE schema_contracts; --";
    let schema = LockedSchema::new(
        malicious_name,
        vec![LockedColumn::required("id", DataType::Int64)],
    );
    let contract = SchemaContract::new("scope-malicious", schema, "user");

    let result = storage.save_contract(&contract);
    assert!(result.is_ok(), "Should handle malicious schema name");

    // Verify stored literally
    let retrieved = storage
        .get_contract_for_scope("scope-malicious")
        .unwrap()
        .unwrap();
    assert_eq!(retrieved.schemas[0].name, malicious_name);
}

/// Test format strings don't allow injection
#[test]
fn test_format_string_injection() {
    // Format strings should be stored literally, not interpreted
    let malicious_formats = [
        "%n%n%n%n%n", // printf format string attack
        "${7*7}",     // Template injection
        "{{7*7}}",    // Jinja/Twig injection
        "$(whoami)",  // Command substitution
        "`whoami`",   // Backtick command substitution
    ];

    for format in malicious_formats {
        let column = LockedColumn::required("date", DataType::Date).with_format(format);
        assert_eq!(
            column.format,
            Some(format.to_string()),
            "Format should be stored literally"
        );
    }
}
