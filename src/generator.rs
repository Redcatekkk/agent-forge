use crate::models::{AnalysisReport, Argument, EntryPoint};
use anyhow::Result;
use handlebars::Handlebars;
use serde::Serialize;
use serde_json::{Map, Value, json};

const SKILL_TEMPLATE: &str = include_str!("../templates/SKILL.md.hbs");
const MCP_TEMPLATE: &str = include_str!("../templates/mcp-config.json.hbs");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedArtifacts {
    pub skill_md: String,
    pub mcp_config: String,
}

#[derive(Debug, Default, Clone)]
pub struct Generator;

#[derive(Debug, Serialize)]
struct TemplateContext<'a> {
    report: &'a AnalysisReport,
    entry_point_count: usize,
    has_warnings: bool,
    should_condense: bool,
    tools: Vec<ToolTemplate>,
}

#[derive(Debug, Serialize)]
struct ToolTemplate {
    name: String,
    description: String,
    framework: String,
    method: Option<String>,
    route: Option<String>,
    file: String,
    input_schema_json: String,
}

impl Generator {
    pub fn render(&self, report: &AnalysisReport) -> Result<RenderedArtifacts> {
        let mut handlebars = Handlebars::new();
        handlebars.register_template_string("skill", SKILL_TEMPLATE)?;
        handlebars.register_template_string("mcp", MCP_TEMPLATE)?;

        let tools = report.entry_points.iter().map(tool_from_entry).collect();
        let context = TemplateContext {
            report,
            entry_point_count: report.entry_points.len(),
            has_warnings: !report.warnings.is_empty(),
            should_condense: report.files_scanned >= report.condensation_threshold,
            tools,
        };

        Ok(RenderedArtifacts {
            skill_md: handlebars.render("skill", &context)?,
            mcp_config: handlebars.render("mcp", &context)?,
        })
    }
}

fn tool_from_entry(entry: &EntryPoint) -> ToolTemplate {
    ToolTemplate {
        name: sanitize_tool_name(&entry.name),
        description: entry.intent.clone(),
        framework: entry.framework.clone(),
        method: entry.method.clone(),
        route: entry.route.clone(),
        file: entry.file.clone(),
        input_schema_json: serde_json::to_string_pretty(&input_schema(&entry.args))
            .expect("JSON schema serialization cannot fail"),
    }
}

fn input_schema(args: &[Argument]) -> Value {
    let mut properties = Map::new();
    let mut required = Vec::new();

    for arg in args {
        if arg.required {
            required.push(Value::String(arg.name.clone()));
        }

        let mut prop = Map::new();
        prop.insert(
            "type".into(),
            Value::String(json_schema_type(arg.type_hint.as_deref()).into()),
        );
        if let Some(description) = &arg.description {
            prop.insert("description".into(), Value::String(description.clone()));
        }
        properties.insert(arg.name.clone(), Value::Object(prop));
    }

    json!({
        "type": "object",
        "properties": properties,
        "required": required
    })
}

fn json_schema_type(type_hint: Option<&str>) -> &'static str {
    match type_hint.unwrap_or("").to_lowercase().as_str() {
        "integer" | "int" | "i32" | "i64" | "long" => "integer",
        "number" | "float" | "double" | "f32" | "f64" => "number",
        "boolean" | "bool" => "boolean",
        "array" | "list" | "vec" => "array",
        "object" | "dict" | "map" => "object",
        _ => "string",
    }
}

fn sanitize_tool_name(name: &str) -> String {
    name.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}
