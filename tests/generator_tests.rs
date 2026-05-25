use agent_forge::generator::{Generator, RenderedArtifacts};
use agent_forge::models::{AnalysisReport, Argument, EntryPoint};
use agent_forge::scanner::AnalysisSource;
use serde_json::json;

#[test]
fn renders_skill_and_mcp_config_with_json_schema_arguments() {
    let report = AnalysisReport {
        repo_name: "payments".into(),
        source: AnalysisSource::PatternScan,
        target_dir: "C:/repos/payments".into(),
        files_scanned: 128,
        condensation_threshold: 40,
        entry_points: vec![EntryPoint {
            name: "createCharge".into(),
            intent: "Create a card charge".into(),
            framework: "Express".into(),
            method: Some("POST".into()),
            route: Some("/charges".into()),
            file: "src/server.js".into(),
            args: vec![Argument {
                name: "amount".into(),
                required: true,
                type_hint: Some("number".into()),
                description: Some("Charge amount in cents".into()),
            }],
            docstring: Some("Create a card charge".into()),
        }],
        warnings: vec!["No OpenAPI document found".into()],
    };

    let RenderedArtifacts {
        skill_md,
        mcp_config,
    } = Generator::default().render(&report).unwrap();
    let config: serde_json::Value = serde_json::from_str(&mcp_config).unwrap();

    assert!(skill_md.contains("## Core Purpose"));
    assert!(skill_md.contains("## Context Condensation"));
    assert!(skill_md.contains("Start with this `SKILL.md`"));
    assert!(skill_md.contains("## Critical Paths"));
    assert!(skill_md.contains("Create a card charge"));
    assert_eq!(
        config["mcpServers"]["payments"]["command"],
        json!("agent-forge")
    );
    assert_eq!(
        config["mcpServers"]["payments"]["args"],
        json!(["serve", "C:/repos/payments"])
    );
    assert_eq!(config["tools"][0]["name"], json!("createCharge"));
    assert_eq!(
        config["tools"][0]["inputSchema"]["properties"]["amount"]["type"],
        json!("number")
    );
    assert_eq!(
        config["tools"][0]["inputSchema"]["required"][0],
        json!("amount")
    );
}
