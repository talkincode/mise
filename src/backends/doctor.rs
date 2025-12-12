//! Doctor - Dependency checking

use anyhow::Result;

use crate::backends::ast_grep::get_ast_grep_command;
use crate::backends::rg::is_rg_available;
use crate::core::model::{Confidence, Kind, MiseError, ResultItem, ResultSet, SourceMode};
use crate::core::render::{OutputFormat, Renderer};
use crate::core::util::command_exists;

/// Dependency status
#[derive(Debug, Clone)]
pub struct DependencyStatus {
    pub name: String,
    pub available: bool,
    pub command: Option<String>,
    pub required: bool,
    pub notes: Option<String>,
}

impl DependencyStatus {
    pub fn to_result_item(&self) -> ResultItem {
        let status = if self.available { "✓" } else { "✗" };
        let required = if self.required {
            "required"
        } else {
            "optional"
        };

        let message = format!(
            "{} {} ({}) - {}",
            status,
            self.name,
            required,
            self.command
                .as_ref()
                .map(|c| format!("found: {}", c))
                .unwrap_or_else(|| "not found".to_string())
        );

        let mut item = ResultItem {
            kind: if self.available {
                Kind::File
            } else {
                Kind::Error
            },
            path: None,
            range: None,
            excerpt: Some(message),
            confidence: if self.available || self.required {
                Confidence::High
            } else {
                Confidence::Low
            },
            source_mode: SourceMode::Scan,
            meta: Default::default(),
            errors: Vec::new(),
        };

        if !self.available && self.required {
            item.errors.push(MiseError::new(
                "MISSING_DEPENDENCY",
                format!("{} is required but not found", self.name),
            ));
        }

        if let Some(notes) = &self.notes {
            item.excerpt = Some(format!(
                "{}\n  Note: {}",
                item.excerpt.unwrap_or_default(),
                notes
            ));
        }

        item
    }
}

/// Check all dependencies
pub fn check_dependencies() -> Vec<DependencyStatus> {
    let mut deps = Vec::new();

    // ripgrep (required for match command)
    deps.push(DependencyStatus {
        name: "ripgrep".to_string(),
        available: is_rg_available(),
        command: if is_rg_available() {
            Some("rg".to_string())
        } else {
            None
        },
        required: true,
        notes: Some("Install: brew install ripgrep / cargo install ripgrep".to_string()),
    });

    // ast-grep (required for ast command)
    let ast_grep_cmd = get_ast_grep_command();
    deps.push(DependencyStatus {
        name: "ast-grep".to_string(),
        available: ast_grep_cmd.is_some(),
        command: ast_grep_cmd.map(|s| s.to_string()),
        required: true,
        notes: Some("Install: cargo install ast-grep / npm install -g @ast-grep/cli".to_string()),
    });

    // watchexec (optional, for watch command)
    let watchexec_available = command_exists("watchexec");
    deps.push(DependencyStatus {
        name: "watchexec".to_string(),
        available: watchexec_available,
        command: if watchexec_available {
            Some("watchexec".to_string())
        } else {
            None
        },
        required: false,
        notes: Some("Install: brew install watchexec / cargo install watchexec-cli".to_string()),
    });

    deps
}

/// Run the doctor command
pub fn run_doctor(format: OutputFormat) -> Result<()> {
    let deps = check_dependencies();

    let mut result_set = ResultSet::new();
    for dep in deps {
        result_set.push(dep.to_result_item());
    }

    let renderer = Renderer::new(format);
    println!("{}", renderer.render(&result_set));

    // Return error if any required dependency is missing
    let missing_required: Vec<_> = check_dependencies()
        .into_iter()
        .filter(|d| d.required && !d.available)
        .collect();

    if !missing_required.is_empty() {
        eprintln!("\n⚠️  Some required dependencies are missing!");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_dependencies() {
        let deps = check_dependencies();
        assert!(!deps.is_empty());

        // Should have at least ripgrep, ast-grep, and watchexec
        let names: Vec<_> = deps.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"ripgrep"));
        assert!(names.contains(&"ast-grep"));
        assert!(names.contains(&"watchexec"));
    }
}
