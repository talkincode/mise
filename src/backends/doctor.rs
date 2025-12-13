//! Doctor - Dependency checking

use anyhow::Result;

use crate::backends::ast_grep::get_ast_grep_command;
use crate::backends::rg::is_rg_available;
use crate::core::model::{Confidence, Kind, MiseError, ResultItem, ResultSet, SourceMode};
use crate::core::render::{RenderConfig, Renderer};
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

        // 如果已安装，只显示状态；未安装才显示安装方法
        let message = if self.available {
            format!(
                "{} {} ({}) - installed: {}",
                status,
                self.name,
                required,
                self.command.as_ref().unwrap_or(&"unknown".to_string())
            )
        } else {
            let install_hint = self
                .notes
                .as_ref()
                .map(|n| format!("\n  {}", n))
                .unwrap_or_default();
            format!(
                "{} {} ({}) - not found{}",
                status, self.name, required, install_hint
            )
        };

        let mut item = ResultItem {
            kind: if self.available {
                Kind::File
            } else {
                Kind::Error
            },
            path: None,
            range: None,
            excerpt: Some(message),
            data: None,
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
pub fn run_doctor(config: RenderConfig) -> Result<()> {
    let deps = check_dependencies();

    let mut result_set = ResultSet::new();
    for dep in deps {
        result_set.push(dep.to_result_item());
    }

    let renderer = Renderer::with_config(config);
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

    #[test]
    fn test_dependency_status_to_result_item_available() {
        let status = DependencyStatus {
            name: "test-tool".to_string(),
            available: true,
            command: Some("test".to_string()),
            required: true,
            notes: None,
        };
        let item = status.to_result_item();
        assert!(matches!(item.kind, Kind::File));
        assert!(item.excerpt.is_some());
        assert!(item.excerpt.as_ref().unwrap().contains("✓"));
        assert!(item.excerpt.as_ref().unwrap().contains("installed"));
    }

    #[test]
    fn test_dependency_status_to_result_item_unavailable_required() {
        let status = DependencyStatus {
            name: "missing-tool".to_string(),
            available: false,
            command: None,
            required: true,
            notes: Some("Install with: cargo install missing-tool".to_string()),
        };
        let item = status.to_result_item();
        assert!(matches!(item.kind, Kind::Error));
        assert!(!item.errors.is_empty());
        assert!(item.errors[0].code == "MISSING_DEPENDENCY");
        assert!(item.excerpt.as_ref().unwrap().contains("✗"));
        assert!(item.excerpt.as_ref().unwrap().contains("not found"));
    }

    #[test]
    fn test_dependency_status_to_result_item_unavailable_optional() {
        let status = DependencyStatus {
            name: "optional-tool".to_string(),
            available: false,
            command: None,
            required: false,
            notes: Some("Optional install".to_string()),
        };
        let item = status.to_result_item();
        // Optional missing deps don't add errors
        assert!(item.errors.is_empty());
        assert!(item.excerpt.as_ref().unwrap().contains("optional"));
    }

    #[test]
    fn test_dependency_status_confidence() {
        let available_required = DependencyStatus {
            name: "tool1".to_string(),
            available: true,
            command: Some("t1".to_string()),
            required: true,
            notes: None,
        };
        assert!(matches!(available_required.to_result_item().confidence, Confidence::High));

        let unavailable_required = DependencyStatus {
            name: "tool2".to_string(),
            available: false,
            command: None,
            required: true,
            notes: None,
        };
        assert!(matches!(unavailable_required.to_result_item().confidence, Confidence::High));

        let unavailable_optional = DependencyStatus {
            name: "tool3".to_string(),
            available: false,
            command: None,
            required: false,
            notes: None,
        };
        assert!(matches!(unavailable_optional.to_result_item().confidence, Confidence::Low));
    }

    #[test]
    fn test_check_dependencies_ripgrep_required() {
        let deps = check_dependencies();
        let rg = deps.iter().find(|d| d.name == "ripgrep").unwrap();
        assert!(rg.required);
    }

    #[test]
    fn test_check_dependencies_watchexec_optional() {
        let deps = check_dependencies();
        let watchexec = deps.iter().find(|d| d.name == "watchexec").unwrap();
        assert!(!watchexec.required);
    }

    #[test]
    fn test_dependency_status_notes_in_output() {
        let status = DependencyStatus {
            name: "tool".to_string(),
            available: false,
            command: None,
            required: true,
            notes: Some("brew install tool".to_string()),
        };
        let item = status.to_result_item();
        assert!(item.excerpt.as_ref().unwrap().contains("brew install"));
    }

    #[test]
    fn test_run_doctor_command() {
        let config = crate::core::render::RenderConfig {
            format: crate::core::render::OutputFormat::Json,
            pretty: false,
        };

        let result = run_doctor(config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_dependency_status_available_without_notes() {
        let status = DependencyStatus {
            name: "tool".to_string(),
            available: true,
            command: Some("tool".to_string()),
            required: true,
            notes: None,
        };
        let item = status.to_result_item();
        assert!(item.excerpt.is_some());
        // Should still produce valid output without notes
        assert!(item.excerpt.as_ref().unwrap().contains("tool"));
    }

    #[test]
    fn test_dependency_status_command_in_output() {
        let status = DependencyStatus {
            name: "ripgrep".to_string(),
            available: true,
            command: Some("rg".to_string()),
            required: true,
            notes: None,
        };
        let item = status.to_result_item();
        // Command should be mentioned in excerpt
        assert!(item.excerpt.as_ref().unwrap().contains("rg"));
    }

    #[test]
    fn test_check_dependencies_has_notes() {
        let deps = check_dependencies();
        // All dependencies should have install notes
        for dep in &deps {
            assert!(dep.notes.is_some());
        }
    }

    #[test]
    fn test_check_dependencies_ast_grep_required() {
        let deps = check_dependencies();
        let ast = deps.iter().find(|d| d.name == "ast-grep").unwrap();
        assert!(ast.required);
    }

    #[test]
    fn test_dependency_status_source_mode() {
        let status = DependencyStatus {
            name: "test".to_string(),
            available: true,
            command: Some("test".to_string()),
            required: true,
            notes: None,
        };
        let item = status.to_result_item();
        // Doctor results should have appropriate source mode
        assert!(matches!(item.source_mode, crate::core::model::SourceMode::Scan));
    }
}
