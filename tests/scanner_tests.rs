use agent_forge::scanner::{AnalysisSource, Scanner};
use std::fs;
use tempfile::tempdir;

#[test]
fn prioritizes_openapi_over_source_patterns() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("openapi.yaml"),
        r#"
openapi: 3.0.0
info:
  title: Billing API
  version: 1.0.0
paths:
  /invoices/{id}:
    get:
      operationId: getInvoice
      summary: Fetch one invoice
      parameters:
        - name: id
          in: path
          required: true
          schema:
            type: string
"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("server.js"),
        r#"app.post("/ignored", function ignored(req, res) { res.send("nope") })"#,
    )
    .unwrap();

    let result = Scanner::default().scan(dir.path()).unwrap();

    assert_eq!(result.source, AnalysisSource::OpenApi);
    assert_eq!(result.files_scanned, 2);
    assert_eq!(result.entry_points.len(), 1);
    assert_eq!(result.entry_points[0].name, "getInvoice");
    assert_eq!(result.entry_points[0].args[0].name, "id");
}

#[test]
fn detects_common_framework_entry_points() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("app.py"),
        r#"
from fastapi import FastAPI
app = FastAPI()

@app.post("/users/{user_id}")
def update_user(user_id: str, active: bool):
    """Update whether a user is active."""
    return {"ok": True}
"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("main.go"),
        r#"
package main

func register(r *gin.Engine) {
    r.GET("/health", healthCheck)
}

// healthCheck reports service readiness.
func healthCheck(c *gin.Context) {}
"#,
    )
    .unwrap();

    let result = Scanner::default().scan(dir.path()).unwrap();
    let names: Vec<_> = result
        .entry_points
        .iter()
        .map(|entry| entry.name.as_str())
        .collect();

    assert!(names.contains(&"update_user"));
    assert!(names.contains(&"healthCheck"));
    assert_eq!(result.files_scanned, 2);
    assert!(
        result
            .entry_points
            .iter()
            .any(|entry| entry.framework == "FastAPI" && entry.intent.contains("Update whether"))
    );
}

#[test]
fn ignores_graphql_words_inside_non_schema_source_files() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("lib.rs"),
        "pub const SAMPLE: &str = \"type Query { user(id: ID!): User }\";",
    )
    .unwrap();

    let result = Scanner::default().scan(dir.path()).unwrap();

    assert!(result.entry_points.is_empty());
}

#[test]
fn only_graphql_query_and_mutation_fields_become_entry_points() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("schema.graphql"),
        r#"
type Query {
  user(id: ID!): User
}

type Mutation {
  deactivateUser(id: ID!, reason: String): User
}

type User {
  id: ID!
  email: String!
  active: Boolean!
}
"#,
    )
    .unwrap();

    let result = Scanner::default().scan(dir.path()).unwrap();
    let names: Vec<_> = result
        .entry_points
        .iter()
        .map(|entry| entry.name.as_str())
        .collect();

    assert_eq!(names, vec!["deactivateUser", "user"]);
}
