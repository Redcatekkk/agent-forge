use crate::models::{AnalysisReport, Argument, EntryPoint};
use crate::scanner::AnalysisSource;
use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize)]
struct OpenApiDoc {
    paths: BTreeMap<String, PathItem>,
}

#[derive(Debug, Deserialize, Default)]
struct PathItem {
    get: Option<Operation>,
    post: Option<Operation>,
    put: Option<Operation>,
    patch: Option<Operation>,
    delete: Option<Operation>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Operation {
    operation_id: Option<String>,
    summary: Option<String>,
    description: Option<String>,
    parameters: Option<Vec<Parameter>>,
    request_body: Option<Value>,
}

#[derive(Debug, Deserialize, Default)]
struct Parameter {
    name: String,
    required: Option<bool>,
    description: Option<String>,
    schema: Option<Value>,
}

pub fn parse_file(path: &Path) -> Result<AnalysisReport> {
    let raw = fs::read_to_string(path)?;
    let doc: OpenApiDoc = if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
        serde_json::from_str(&raw).context("invalid OpenAPI JSON")?
    } else {
        serde_yaml::from_str(&raw).context("invalid OpenAPI YAML")?
    };

    let mut entry_points = Vec::new();
    for (route, item) in doc.paths {
        for (method, operation) in [
            ("GET", item.get),
            ("POST", item.post),
            ("PUT", item.put),
            ("PATCH", item.patch),
            ("DELETE", item.delete),
        ] {
            if let Some(operation) = operation {
                let name = operation
                    .operation_id
                    .clone()
                    .unwrap_or_else(|| fallback_operation_name(method, &route));
                let intent = operation
                    .summary
                    .clone()
                    .or(operation.description.clone())
                    .unwrap_or_else(|| fallback_operation_name(method, &route));
                let mut args = operation
                    .parameters
                    .unwrap_or_default()
                    .into_iter()
                    .map(|param| Argument {
                        name: param.name,
                        required: param.required.unwrap_or(false),
                        type_hint: param.schema.as_ref().and_then(schema_type),
                        description: param.description,
                    })
                    .collect::<Vec<_>>();

                if operation.request_body.is_some() {
                    args.push(Argument {
                        name: "body".into(),
                        required: true,
                        type_hint: Some("object".into()),
                        description: Some("Request body".into()),
                    });
                }

                entry_points.push(EntryPoint {
                    name,
                    intent: intent.clone(),
                    framework: "OpenAPI".into(),
                    method: Some(method.into()),
                    route: Some(route.clone()),
                    file: path.to_string_lossy().replace('\\', "/"),
                    args,
                    docstring: Some(intent),
                });
            }
        }
    }

    Ok(AnalysisReport {
        repo_name: "repository".into(),
        source: AnalysisSource::OpenApi,
        target_dir: path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_string_lossy()
            .replace('\\', "/"),
        files_scanned: 1,
        condensation_threshold: crate::scanner::DEFAULT_CONDENSATION_THRESHOLD,
        entry_points,
        warnings: Vec::new(),
    })
}

fn schema_type(schema: &Value) -> Option<String> {
    schema
        .get("type")
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn fallback_operation_name(method: &str, route: &str) -> String {
    let suffix = route
        .split('/')
        .filter(|part| !part.is_empty())
        .map(|part| part.trim_matches('{').trim_matches('}'))
        .collect::<Vec<_>>()
        .join("_");
    format!("{}_{}", method.to_lowercase(), suffix)
}
