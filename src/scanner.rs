use crate::models::{AnalysisReport, Argument, EntryPoint};
use crate::openapi;
use anyhow::{Context, Result};
use ignore::WalkBuilder;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const DEFAULT_CONDENSATION_THRESHOLD: u64 = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnalysisSource {
    OpenApi,
    PatternScan,
}

#[derive(Debug, Default, Clone)]
pub struct Scanner;

impl Scanner {
    pub fn scan(&self, root: &Path) -> Result<AnalysisReport> {
        self.scan_with_progress(root, |_| {})
    }

    pub fn scan_with_progress<F>(&self, root: &Path, mut on_file: F) -> Result<AnalysisReport>
    where
        F: FnMut(&Path),
    {
        let repo_name = root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("repository")
            .to_string();
        let files_scanned = count_context_files(root)?;
        let target_dir = root.to_string_lossy().replace('\\', "/");

        if let Some(openapi_path) = find_openapi_file(root)? {
            let mut report = openapi::parse_file(&openapi_path)
                .with_context(|| format!("failed to parse {}", openapi_path.display()))?;
            report.repo_name = repo_name;
            report.target_dir = target_dir;
            report.files_scanned = files_scanned;
            report.condensation_threshold = DEFAULT_CONDENSATION_THRESHOLD;
            return Ok(report);
        }

        let mut entry_points = Vec::new();
        let mut warnings = vec!["No OpenAPI document found; using local pattern scanner".into()];

        for result in WalkBuilder::new(root)
            .hidden(false)
            .ignore(true)
            .git_ignore(true)
            .build()
        {
            let dent = result?;
            let path = dent.path();
            if !path.is_file() || !is_supported_source(path) {
                continue;
            }

            let content = fs::read_to_string(path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            entry_points.extend(scan_source_file(root, path, &content));
            on_file(path);
        }

        entry_points
            .sort_by(|left, right| left.file.cmp(&right.file).then(left.name.cmp(&right.name)));
        entry_points.dedup_by(|left, right| {
            left.name == right.name
                && left.file == right.file
                && left.method == right.method
                && left.route == right.route
        });

        if entry_points.is_empty() {
            warnings.push("No supported framework entry points were detected".into());
        }

        Ok(AnalysisReport {
            repo_name,
            source: AnalysisSource::PatternScan,
            target_dir,
            files_scanned,
            condensation_threshold: DEFAULT_CONDENSATION_THRESHOLD,
            entry_points,
            warnings,
        })
    }
}

pub fn count_candidate_files(root: &Path) -> Result<u64> {
    let mut count = 0;
    for result in WalkBuilder::new(root)
        .hidden(false)
        .ignore(true)
        .git_ignore(true)
        .build()
    {
        let dent = result?;
        let path = dent.path();
        if path.is_file() && is_supported_source(path) {
            count += 1;
        }
    }
    Ok(count)
}

pub fn count_context_files(root: &Path) -> Result<u64> {
    let mut count = 0;
    for result in WalkBuilder::new(root)
        .hidden(false)
        .ignore(true)
        .git_ignore(true)
        .build()
    {
        let dent = result?;
        if dent.path().is_file() {
            count += 1;
        }
    }
    Ok(count)
}

fn find_openapi_file(root: &Path) -> Result<Option<PathBuf>> {
    let names = [
        "openapi.yaml",
        "openapi.yml",
        "openapi.json",
        "swagger.yaml",
        "swagger.yml",
        "swagger.json",
    ];

    for result in WalkBuilder::new(root)
        .hidden(false)
        .ignore(true)
        .git_ignore(true)
        .build()
    {
        let dent = result?;
        let path = dent.path();
        if path.is_file() {
            let file_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            if names
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(file_name))
            {
                return Ok(Some(path.to_path_buf()));
            }
        }
    }

    Ok(None)
}

fn is_supported_source(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("js" | "jsx" | "ts" | "tsx" | "py" | "go" | "java" | "graphql" | "gql" | "rs")
    )
}

fn scan_source_file(root: &Path, path: &Path, content: &str) -> Vec<EntryPoint> {
    let file = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");

    let mut entries = Vec::new();
    entries.extend(scan_express(&file, content));
    entries.extend(scan_fastapi(&file, content));
    entries.extend(scan_gin(&file, content));
    entries.extend(scan_spring(&file, content));
    if matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("graphql" | "gql")
    ) {
        entries.extend(scan_graphql(&file, content));
    }
    entries
}

fn scan_express(file: &str, content: &str) -> Vec<EntryPoint> {
    static RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r#"(?s)(?P<comment>/\*\*.*?\*/|//[^\n]*\n)?\s*(?:app|router)\.(?P<method>get|post|put|patch|delete)\(\s*["'`](?P<route>[^"'`]+)["'`]\s*,\s*(?:async\s+)?(?:function\s+(?P<fname>[A-Za-z_][A-Za-z0-9_]*)\s*\((?P<fargs>[^)]*)\)|(?P<handler>[A-Za-z_][A-Za-z0-9_]*))"#)
            .unwrap()
    });

    RE.captures_iter(content)
        .map(|caps| {
            let name = caps
                .name("fname")
                .or_else(|| caps.name("handler"))
                .map(|m| m.as_str())
                .unwrap_or_else(|| caps.name("route").unwrap().as_str().trim_matches('/'))
                .to_string();
            let doc = clean_comment(caps.name("comment").map(|m| m.as_str()).unwrap_or(""));
            EntryPoint {
                intent: intent_from_doc_or_name(doc.as_deref(), &name),
                name,
                framework: "Express".into(),
                method: Some(caps["method"].to_uppercase()),
                route: Some(caps["route"].to_string()),
                file: file.into(),
                args: parse_arg_list(caps.name("fargs").map(|m| m.as_str()).unwrap_or("")),
                docstring: doc,
            }
        })
        .collect()
}

fn scan_fastapi(file: &str, content: &str) -> Vec<EntryPoint> {
    static RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r#"(?s)@(?:\w+\.)?(?:app|router)\.(?P<method>get|post|put|patch|delete)\(\s*["'](?P<route>[^"']+)["'][^\n]*\)\s*(?:async\s+)?def\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)\((?P<args>[^)]*)\):\s*(?:"""(?P<doc_triple>.*?)"""|'''(?P<doc_single>.*?)''')?"#)
            .unwrap()
    });

    RE.captures_iter(content)
        .map(|caps| {
            let doc = caps
                .name("doc_triple")
                .or_else(|| caps.name("doc_single"))
                .map(|doc| doc.as_str().trim().replace('\n', " "));
            let name = caps["name"].to_string();
            EntryPoint {
                intent: intent_from_doc_or_name(doc.as_deref(), &name),
                name,
                framework: "FastAPI".into(),
                method: Some(caps["method"].to_uppercase()),
                route: Some(caps["route"].to_string()),
                file: file.into(),
                args: parse_arg_list(&caps["args"]),
                docstring: doc,
            }
        })
        .collect()
}

fn scan_gin(file: &str, content: &str) -> Vec<EntryPoint> {
    static ROUTE_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r#"\b(?:r|router|engine|group)\.(?P<method>GET|POST|PUT|PATCH|DELETE)\(\s*"(?P<route>[^"]+)"\s*,\s*(?P<handler>[A-Za-z_][A-Za-z0-9_]*)"#)
            .unwrap()
    });

    ROUTE_RE
        .captures_iter(content)
        .map(|caps| {
            let name = caps["handler"].to_string();
            let doc = extract_go_doc(content, &name);
            EntryPoint {
                intent: intent_from_doc_or_name(doc.as_deref(), &name),
                name,
                framework: "Gin".into(),
                method: Some(caps["method"].to_string()),
                route: Some(caps["route"].to_string()),
                file: file.into(),
                args: Vec::new(),
                docstring: doc,
            }
        })
        .collect()
}

fn scan_spring(file: &str, content: &str) -> Vec<EntryPoint> {
    static RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r#"(?s)(?P<comment>/\*\*.*?\*/)?\s*@(?P<anno>GetMapping|PostMapping|PutMapping|PatchMapping|DeleteMapping|RequestMapping)(?:\((?P<meta>[^)]*)\))?\s*(?:public|private|protected)?\s*[\w<>, ?]+\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)\((?P<args>[^)]*)\)"#)
            .unwrap()
    });

    RE.captures_iter(content)
        .map(|caps| {
            let name = caps["name"].to_string();
            let doc = clean_comment(caps.name("comment").map(|m| m.as_str()).unwrap_or(""));
            EntryPoint {
                intent: intent_from_doc_or_name(doc.as_deref(), &name),
                name,
                framework: "Spring Boot".into(),
                method: Some(spring_method(&caps["anno"]).into()),
                route: extract_first_string(caps.name("meta").map(|m| m.as_str()).unwrap_or("")),
                file: file.into(),
                args: parse_arg_list(&caps["args"]),
                docstring: doc,
            }
        })
        .collect()
}

fn scan_graphql(file: &str, content: &str) -> Vec<EntryPoint> {
    static RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r#"(?m)^\s*(?P<name>[A-Za-z_][A-Za-z0-9_]*)\s*(?:\((?P<args>[^)]*)\))?\s*:\s*(?P<ret>[A-Za-z_\[\]!][A-Za-z0-9_\[\]!]*)"#)
            .unwrap()
    });
    static BLOCK_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r#"(?s)type\s+(?:Query|Mutation)\s*\{(?P<body>.*?)\}"#).unwrap());

    if !(content.contains("type Query") || content.contains("type Mutation")) {
        return Vec::new();
    }

    let query_and_mutation_body = BLOCK_RE
        .captures_iter(content)
        .filter_map(|caps| caps.name("body").map(|body| body.as_str()))
        .collect::<Vec<_>>()
        .join("\n");

    RE.captures_iter(&query_and_mutation_body)
        .filter_map(|caps| {
            let name = caps.name("name")?.as_str().to_string();
            if matches!(name.as_str(), "type" | "schema" | "query" | "mutation") {
                return None;
            }
            Some(EntryPoint {
                intent: intent_from_doc_or_name(None, &name),
                name,
                framework: "GraphQL".into(),
                method: None,
                route: None,
                file: file.into(),
                args: parse_arg_list(caps.name("args").map(|m| m.as_str()).unwrap_or("")),
                docstring: None,
            })
        })
        .collect()
}

fn spring_method(annotation: &str) -> &'static str {
    match annotation {
        "GetMapping" => "GET",
        "PostMapping" => "POST",
        "PutMapping" => "PUT",
        "PatchMapping" => "PATCH",
        "DeleteMapping" => "DELETE",
        _ => "ANY",
    }
}

fn extract_first_string(meta: &str) -> Option<String> {
    static RE: Lazy<Regex> = Lazy::new(|| Regex::new(r#""([^"]+)""#).unwrap());
    RE.captures(meta).map(|caps| caps[1].to_string())
}

fn extract_go_doc(content: &str, name: &str) -> Option<String> {
    let pattern = format!(
        r#"(?m)((?://[^\n]*\n)+)\s*func\s+{}\b"#,
        regex::escape(name)
    );
    let re = Regex::new(&pattern).ok()?;
    re.captures(content)
        .and_then(|caps| caps.get(1))
        .and_then(|comment| clean_comment(comment.as_str()))
}

fn clean_comment(comment: &str) -> Option<String> {
    let cleaned = comment
        .lines()
        .map(|line| {
            line.trim()
                .trim_start_matches("/**")
                .trim_start_matches("/*")
                .trim_start_matches("//")
                .trim_start_matches('*')
                .trim_end_matches("*/")
                .trim()
        })
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

fn intent_from_doc_or_name(doc: Option<&str>, name: &str) -> String {
    doc.filter(|value| !value.trim().is_empty())
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|| humanize_identifier(name))
}

fn humanize_identifier(name: &str) -> String {
    let mut output = String::new();
    for (idx, ch) in name.chars().enumerate() {
        if idx > 0 && ch.is_uppercase() {
            output.push(' ');
        }
        output.push(ch);
    }
    output.replace('_', " ")
}

fn parse_arg_list(args: &str) -> Vec<Argument> {
    args.split(',')
        .filter_map(|arg| {
            let mut value = arg.trim();
            if value.is_empty()
                || matches!(value, "self" | "req" | "res" | "next" | "c *gin.Context")
            {
                return None;
            }
            if let Some((name, type_hint)) = value.split_once(':') {
                return Some(Argument {
                    name: clean_arg_name(name),
                    required: !type_hint.contains("Optional") && !type_hint.contains('?'),
                    type_hint: Some(clean_type_hint(type_hint)),
                    description: None,
                });
            }
            if value.contains(' ') {
                value = value.split_whitespace().last().unwrap_or(value);
            }
            Some(Argument {
                name: clean_arg_name(value),
                required: !value.contains('?'),
                type_hint: None,
                description: None,
            })
        })
        .filter(|arg| !arg.name.is_empty())
        .collect()
}

fn clean_arg_name(name: &str) -> String {
    name.trim()
        .trim_start_matches('@')
        .trim_start_matches("PathVariable")
        .trim_start_matches("RequestParam")
        .trim_matches(|ch: char| !ch.is_alphanumeric() && ch != '_')
        .trim_end_matches('?')
        .to_string()
}

fn clean_type_hint(type_hint: &str) -> String {
    type_hint
        .split('=')
        .next()
        .unwrap_or(type_hint)
        .trim()
        .trim_end_matches('!')
        .trim()
        .to_string()
}
