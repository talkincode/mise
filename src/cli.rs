//! CLI module - Command-line interface definitions and handlers

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::core::render::OutputFormat;

/// mise - a unified CLI for scanning files, managing anchors, and searching code.
#[derive(Parser, Debug)]
#[command(name = "mise")]
#[command(
    author,
    version,
    about,
    long_about = r#"mise emits a unified, machine-readable result model for every command.

Each command prints a ResultSet in the selected format (default: jsonl).

Output formats:
- jsonl: one JSON object per line (best for piping into tools/LLMs)
- json: a single JSON array
- md: human-friendly Markdown
- raw: excerpts only (unstable; intended for debugging)

Examples:
    mise scan --type file
    mise match "TODO|FIXME" src
    mise extract README.md --lines 1:40
    mise anchor list --tag chapter
    mise flow writing --anchor intro --max-items 12
"#
)]
pub struct Cli {
    /// Root directory for all operations.
    #[arg(
        long,
        global = true,
        default_value = ".",
        value_name = "ROOT",
        long_help = "Root directory for all operations (defaults to the current directory).\n\n\
All paths emitted in results are relative to this root, and most positional paths/\n\
scopes are interpreted relative to it."
    )]
    pub root: PathBuf,

    /// Output format (jsonl/json/md/raw).
    #[arg(
        long,
        global = true,
        default_value = "jsonl",
        value_name = "FORMAT",
        long_help = "Select the output format for ResultSet.\n\n\
Supported values:\n\
- jsonl (default)\n\
- json\n\
- md (markdown)\n\
- raw\n\n\
Tip: Prefer jsonl when you want stable, line-oriented output for piping and prompts."
    )]
    pub format: String,

    /// Disable colored output (when applicable).
    #[arg(
        long,
        global = true,
        long_help = "Disable colored output. This is useful when piping to files or when your\n\
terminal does not support ANSI colors."
    )]
    pub no_color: bool,

    /// Quiet mode (minimal output).
    #[arg(
        short,
        long,
        global = true,
        long_help = "Reduce non-essential output. Note: machine-readable results are still\n\
printed to stdout unless a command explicitly suppresses them."
    )]
    pub quiet: bool,

    /// Verbose mode (more diagnostics).
    #[arg(
        short,
        long,
        global = true,
        long_help = "Enable more detailed diagnostics. This is intended for debugging and\n\
may increase stderr output."
    )]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Scan the filesystem and output a stable list of paths.
    #[command(
        long_about = "Scan the filesystem under ROOT (or an optional --scope) and emit one\n\
ResultItem per discovered entry. Output is sorted for stability.\n\n\
Use this when you need a reliable file/dir inventory to feed into other tools or prompts.\n\n\
Examples:\n\
  mise scan --type file\n\
  mise scan --type dir --max-depth 2\n\
  mise scan --scope src --hidden --no-ignore\n"
    )]
    Scan {
        /// Limit scanning to a subdirectory under ROOT.
        #[arg(
            long,
            value_name = "PATH",
            long_help = "Limit scanning to a subdirectory under ROOT.\n\n\
If omitted, ROOT is scanned."
        )]
        scope: Option<PathBuf>,

        /// Maximum directory depth from the scan start.
        #[arg(
            long,
            value_name = "N",
            long_help = "Maximum directory depth from the scan start (ROOT or --scope).\n\n\
If omitted, scan traverses all depths."
        )]
        max_depth: Option<usize>,

        /// Include hidden files/directories (dotfiles).
        #[arg(
            long,
            long_help = "Include hidden files and directories (dotfiles).\n\n\
By default, hidden entries are skipped."
        )]
        hidden: bool,

        /// Disable .gitignore and other ignore rules.
        #[arg(
            long,
            long_help = "Disable respect for ignore files (.gitignore, .ignore, global ignores).\n\n\
Use this for a raw scan that includes all paths, even those normally ignored."
        )]
        no_ignore: bool,

        /// Filter results by entry type.
        #[arg(
            long,
            value_parser = ["file", "dir"],
            value_name = "TYPE",
            long_help = "Filter results by entry type.\n\n\
Allowed values: file, dir.\n\n\
If omitted, both files and directories may be returned."
        )]
        r#type: Option<String>,
    },

    /// Find files by substring match (built on top of scan).
    #[command(
        long_about = r#"Find files under ROOT (or --scope) whose paths contain PATTERN
(case-insensitive substring match).

This is a lightweight alternative to full-text search when you only need path filtering.

Examples:
    mise find cargo
    mise find "readme" --scope docs
"#
    )]
    Find {
        /// Substring pattern to match against paths.
        #[arg(value_name = "PATTERN")]
        pattern: Option<String>,

        /// Limit search to a subdirectory under ROOT.
        #[arg(long, value_name = "PATH")]
        scope: Option<PathBuf>,
    },

    /// Extract a line range from a file.
    #[command(
        long_about = "Extract a specific line range from a text file and emit a single Extract\n\
result item containing the excerpt.\n\n\
This is useful for building prompts with precise citations.\n\n\
Examples:\n\
  mise extract README.md --lines 1:40\n\
  mise extract src/main.rs --lines 10:60 --max-bytes 20000\n"
    )]
    Extract {
        /// File path to extract from (relative to ROOT unless absolute).
        #[arg(value_name = "FILE")]
        path: PathBuf,

        /// Line range to extract (1-indexed, format: start:end).
        #[arg(
            long,
            value_name = "START:END",
            long_help = "Line range to extract (1-indexed). Format: start:end.\n\n\
Example: --lines 5:12"
        )]
        lines: String,

        /// Maximum bytes to emit in the excerpt.
        #[arg(
            long,
            default_value = "65536",
            value_name = "BYTES",
            long_help = "Maximum bytes to emit in the excerpt.\n\n\
If the selected range is larger, the excerpt is truncated and the result meta marks it\n\
as truncated."
        )]
        max_bytes: usize,
    },

    /// Manage anchors embedded in text files.
    #[command(
        long_about = "Anchors are lightweight markers embedded in text files:\n\
  <!--Q:begin id=... tags=a,b v=1-->\n\
  ...content...\n\
  <!--Q:end id=...-->\n\n\
Use anchor subcommands to list/get/lint anchors across the workspace."
    )]
    Anchor {
        #[command(subcommand)]
        action: AnchorCommands,
    },

    /// Search file contents using ripgrep (rg).
    #[command(
        long_about = r#"Run ripgrep with JSON output (rg --json) and convert matches into the
unified result model.

If no SCOPE is provided, the search runs under ROOT.

Examples:
    mise match "TODO|FIXME"
    mise match "unsafe" src tests
"#
    )]
    Match {
        /// ripgrep regex pattern.
        #[arg(value_name = "PATTERN")]
        pattern: String,

        /// Optional scope paths (relative to ROOT unless absolute).
        #[arg(value_name = "SCOPE", num_args = 0..)]
        scope: Vec<PathBuf>,
    },

    /// Structural code search using ast-grep (sg/ast-grep).
    #[command(
        long_about = r#"Run ast-grep (sg/ast-grep) and convert structural matches into the
unified result model.

If no SCOPE is provided, the search runs under ROOT.

Examples:
    mise ast "console.log($A)" src
    mise ast "unsafe { $A }"
"#
    )]
    Ast {
        /// ast-grep pattern.
        #[arg(value_name = "PATTERN")]
        pattern: String,

        /// Optional scope paths (relative to ROOT unless absolute).
        #[arg(value_name = "SCOPE", num_args = 0..)]
        scope: Vec<PathBuf>,
    },

    /// Higher-level workflows that combine multiple sources.
    #[command(
        long_about = "Flows are multi-step commands that combine anchors + search + heuristics\n\
to produce a curated ResultSet.\n\n\
Use these when you want prompt-ready evidence rather than raw matches."
    )]
    Flow {
        #[command(subcommand)]
        action: FlowCommands,
    },

    /// Rebuild the .mise cache directory.
    #[command(
        long_about = "Rebuild cached artifacts under .mise/ (e.g., files.jsonl, anchors.jsonl,\n\
meta.json).\n\n\
Use this to speed up repeated workflows or to snapshot workspace state.\n\n\
Example:\n\
  mise rebuild\n"
    )]
    Rebuild,

    /// Check external dependencies and system status.
    #[command(
        long_about = "Check whether required/optional external tools are installed and\n\
discoverable (e.g., rg, sg/ast-grep, watchexec).\n\n\
Example:\n\
  mise doctor\n"
    )]
    Doctor,

    /// Watch for file changes (requires 'watch' feature)
    #[cfg(feature = "watch")]
    Watch {
        /// Command to run on changes
        #[arg(long)]
        cmd: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum AnchorCommands {
    /// List anchors found under ROOT.
    #[command(
        long_about = "Scan text-like files under ROOT, parse anchors, and emit one anchor\n\
result per match.\n\n\
Examples:\n\
  mise anchor list\n\
  mise anchor list --tag chapter\n"
    )]
    List {
        /// Only include anchors containing this tag.
        #[arg(long, value_name = "TAG")]
        tag: Option<String>,
    },

    /// Get a specific anchor by ID.
    #[command(
        long_about = "Find an anchor by its id and emit its content as an anchor result item.\n\
Optionally include neighbor anchors that share tags (useful for context expansion).\n\n\
Examples:\n\
  mise anchor get intro\n\
  mise anchor get intro --with-neighbors 3\n"
    )]
    Get {
        /// Anchor ID.
        #[arg(value_name = "ID")]
        id: String,

        /// Include up to N related anchors as neighbors.
        #[arg(
            long,
            value_name = "N",
            long_help = "Include up to N neighbor anchors that share tags with the target anchor.\n\
These neighbors are returned with lower confidence to signal that they are contextual."
        )]
        with_neighbors: Option<usize>,
    },

    /// Lint anchor markers and report issues.
    #[command(
        long_about = "Validate anchor marker pairing, duplicate IDs, and suspicious anchors\n\
(empty/oversized).\n\n\
This command emits issues as error result items, suitable for CI gating.\n\n\
Example:\n\
  mise anchor lint\n"
    )]
    Lint,
}

#[derive(Subcommand, Debug)]
pub enum FlowCommands {
    /// Gather prompt-ready evidence for a writing task.
    #[command(long_about = "Build a curated ResultSet for writing by combining:\n\
1) the primary anchor (high confidence)\n\
2) related anchors by shared tags (medium confidence)\n\
3) keyword-based ripgrep matches (low confidence)\n\n\
Use this to quickly assemble citations and context for a doc/PR/issue response.\n\n\
Example:\n\
  mise flow writing --anchor intro --max-items 12\n")]
    Writing {
        /// Primary anchor ID.
        #[arg(long, value_name = "ID")]
        anchor: String,

        /// Maximum number of items to return.
        #[arg(long, default_value = "10", value_name = "N")]
        max_items: usize,
    },
}

/// Run the CLI with parsed arguments
pub fn run(cli: Cli) -> Result<()> {
    // Parse output format
    let format: OutputFormat = cli.format.parse().unwrap_or_default();

    // Get absolute root path
    let root = cli.root.canonicalize().unwrap_or(cli.root);

    match cli.command {
        Commands::Scan {
            scope,
            max_depth,
            hidden,
            no_ignore,
            r#type,
        } => crate::backends::scan::run_scan(
            &root,
            scope.as_deref(),
            max_depth,
            hidden,
            !no_ignore,
            r#type.as_deref(),
            format,
        ),

        Commands::Find { pattern, scope } => {
            crate::backends::scan::run_find(&root, pattern.as_deref(), scope.as_deref(), format)
        }

        Commands::Extract {
            path,
            lines,
            max_bytes,
        } => crate::backends::extract::run_extract(&root, &path, &lines, max_bytes, format),

        Commands::Anchor { action } => match action {
            AnchorCommands::List { tag } => {
                crate::anchors::api::run_list(&root, tag.as_deref(), format)
            }
            AnchorCommands::Get { id, with_neighbors } => {
                crate::anchors::api::run_get(&root, &id, with_neighbors, format)
            }
            AnchorCommands::Lint => crate::anchors::lint::run_lint(&root, format),
        },

        Commands::Match { pattern, scope } => {
            crate::backends::rg::run_match(&root, &pattern, &scope, format)
        }

        Commands::Ast { pattern, scope } => {
            crate::backends::ast_grep::run_ast(&root, &pattern, &scope, format)
        }

        Commands::Flow { action } => match action {
            FlowCommands::Writing { anchor, max_items } => {
                crate::flows::writing::run_writing(&root, &anchor, max_items, format)
            }
        },

        Commands::Rebuild => crate::cache::store::run_rebuild(&root, format),

        Commands::Doctor => crate::backends::doctor::run_doctor(format),

        #[cfg(feature = "watch")]
        Commands::Watch { cmd } => crate::backends::watch::run_watch(&root, cmd.as_deref()),
    }
}
