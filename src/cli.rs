//! CLI module - Command-line interface definitions and handlers

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::core::render::{OutputFormat, RenderConfig};

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

    /// Pretty-print JSON/JSONL output with indentation.
    #[arg(
        long,
        global = true,
        long_help = "Pretty-print JSON and JSONL output with indentation for human readability.\n\n\
This is useful when manually inspecting results. Has no effect on md/raw formats."
    )]
    pub pretty: bool,

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

    /// Analyze code dependencies (imports/requires/use statements).
    #[command(
        long_about = r#"Analyze code dependencies to understand "what does this file depend on"
and "what depends on this file".

Supports: Rust (.rs), TypeScript (.ts/.tsx), JavaScript (.js/.jsx), Python (.py)

Output formats:
- jsonl (default): one JSON object per file
- json: complete JSON array
- dot: Graphviz DOT format (pipe to `dot -Tpng` for visualization)
- mermaid: Mermaid diagram syntax (embed in Markdown)
- tree: ASCII tree view (requires a specific file)
- table: ASCII table summary

Examples:
    mise deps src/cli.rs                    # What does cli.rs depend on?
    mise deps src/cli.rs --reverse          # What depends on cli.rs?
    mise deps --deps-format dot | dot -Tpng -o deps.png
    mise deps --deps-format mermaid >> README.md
    mise deps src/cli.rs --deps-format tree
"#
    )]
    Deps {
        /// File to analyze (if omitted, analyzes all files).
        #[arg(value_name = "FILE")]
        file: Option<PathBuf>,

        /// Show reverse dependencies (what depends on this file).
        #[arg(
            long,
            long_help = "Show files that depend on the target file, instead of what the file depends on."
        )]
        reverse: bool,

        /// Output format for deps (jsonl/json/dot/mermaid/tree/table).
        #[arg(
            long = "deps-format",
            value_name = "FORMAT",
            default_value = "jsonl",
            long_help = "Select the output format for dependency analysis.\n\n\
Supported values:\n\
- jsonl (default): one JSON object per file\n\
- json: complete JSON array\n\
- dot: Graphviz DOT format\n\
- mermaid: Mermaid diagram syntax\n\
- tree: ASCII tree (requires file argument)\n\
- table: ASCII table summary"
        )]
        deps_format: String,
    },

    /// Analyze the impact of code changes.
    #[command(
        long_about = r#"Analyze the impact of code changes by combining git diff with
the dependency graph to understand what will be affected.

This is more powerful than plain `git diff` because:
1. Shows direct impacts: files that depend on changed files
2. Shows transitive impacts: files affected through dependency chains
3. Lists affected anchors: code markers that may need attention

Examples:
    mise impact                        # Analyze unstaged changes
    mise impact --staged               # Analyze staged changes
    mise impact --commit abc123        # Analyze a specific commit
    mise impact --diff main..feature   # Compare branches
    mise impact --impact-format summary
"#
    )]
    Impact {
        /// Analyze staged changes instead of unstaged.
        #[arg(
            long,
            long_help = "Analyze staged changes (git diff --staged) instead of unstaged."
        )]
        staged: bool,

        /// Analyze a specific commit.
        #[arg(
            long,
            value_name = "HASH",
            long_help = "Analyze changes from a specific commit.\n\n\
Example: --commit abc123"
        )]
        commit: Option<String>,

        /// Analyze diff between two refs (base..head).
        #[arg(
            long,
            value_name = "BASE..HEAD",
            long_help = "Analyze the diff between two git refs.\n\n\
Example: --diff main..feature"
        )]
        diff: Option<String>,

        /// Maximum depth for transitive impact analysis.
        #[arg(
            long,
            default_value = "3",
            value_name = "N",
            long_help = "Maximum depth for transitive impact analysis.\n\n\
Higher values find more distant impacts but may be slower."
        )]
        max_depth: usize,

        /// Output format for impact (jsonl/json/summary/table).
        #[arg(
            long = "impact-format",
            value_name = "FORMAT",
            default_value = "jsonl",
            long_help = "Select the output format for impact analysis.\n\n\
Supported values:\n\
- jsonl (default): single JSON line with full analysis\n\
- json: pretty-printed JSON\n\
- summary: human-readable summary\n\
- table: ASCII table format"
        )]
        impact_format: String,
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

    /// Watch for file changes and run commands (requires 'watch' feature)
    #[cfg(feature = "watch")]
    #[command(
        long_about = "Watch for file changes in the workspace and automatically run a command.\n\
Uses watchexec as the backend. By default runs `mise rebuild` immediately and on each change.\n\n\
Supported file extensions by default:\n\
  rs, md, txt, py, js, ts, jsx, tsx, json, yaml, yml, toml, html, css, scss\n\n\
Automatically ignores:\n\
  .mise/, .git/, target/, node_modules/, __pycache__/, dist/, build/\n\n\
Examples:\n\
  mise watch                              # Run 'mise rebuild' on changes\n\
  mise watch --cmd 'mise anchor lint'     # Run custom command\n\
  mise watch --clear --restart            # Clear screen, restart if running\n\
  mise watch --exts rs,md --debounce 500  # Custom extensions and debounce\n\
  mise watch --postpone                   # Wait for first change before running\n"
    )]
    Watch {
        /// Command to run on file changes (default: 'mise rebuild')
        #[arg(
            long,
            value_name = "CMD",
            long_help = "Shell command to execute when files change.\n\n\
If not specified, defaults to 'mise rebuild'."
        )]
        cmd: Option<String>,

        /// File extensions to watch (comma-separated)
        #[arg(
            long,
            value_name = "EXTS",
            long_help = "Comma-separated list of file extensions to watch.\n\n\
Default: rs,md,txt,py,js,ts,jsx,tsx,json,yaml,yml,toml,html,css,scss"
        )]
        exts: Option<String>,

        /// Additional paths to ignore (can be used multiple times)
        #[arg(
            long,
            value_name = "PATH",
            action = clap::ArgAction::Append,
            long_help = "Additional paths or patterns to ignore.\n\n\
Can be specified multiple times. Added to the default ignore list."
        )]
        ignore: Vec<String>,

        /// Debounce delay in milliseconds
        #[arg(
            long,
            value_name = "MS",
            long_help = "Time to wait after a file change before running the command.\n\n\
Helps prevent multiple rapid executions during saves."
        )]
        debounce: Option<u64>,

        /// Clear screen before each run
        #[arg(
            long,
            long_help = "Clear the terminal screen before running the command."
        )]
        clear: bool,

        /// Restart command if it's still running
        #[arg(
            long,
            long_help = "If the command is still running when a new change is detected,\n\
terminate it and start a new run."
        )]
        restart: bool,

        /// Wait for first change before running (don't run at startup)
        #[arg(
            long,
            long_help = "By default, the command runs once immediately at startup.\n\
With this option, wait for the first file change before running."
        )]
        postpone: bool,
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

    /// Mark a text block with anchor markers (insert begin/end tags).
    #[command(
        long_about = "Insert anchor markers around a specified line range in a file.\n\
This is useful for AI agents to quickly mark sections of code or documentation.\n\n\
The markers follow the format:\n\
  <!--Q:begin id=xxx tags=a,b v=1-->\n\
  ...content...\n\
  <!--Q:end id=xxx-->\n\n\
Examples:\n\
  mise anchor mark README.md --start 10 --end 25 --id intro\n\
  mise anchor mark src/main.rs --start 1 --end 50 --id main.entry --tags entry,core\n\
  mise anchor mark doc.md --start 5 --end 10 --id sec1 --dry-run\n"
    )]
    Mark {
        /// File path to mark (relative to ROOT).
        #[arg(value_name = "FILE")]
        file: String,

        /// Start line (1-indexed, inclusive).
        #[arg(long, value_name = "LINE")]
        start: u32,

        /// End line (1-indexed, inclusive).
        #[arg(long, value_name = "LINE")]
        end: u32,

        /// Anchor ID.
        #[arg(long, value_name = "ID")]
        id: String,

        /// Tags for categorization (comma-separated).
        #[arg(long, value_name = "TAGS", value_delimiter = ',')]
        tags: Vec<String>,

        /// Version number (default: 1).
        #[arg(long, default_value = "1", value_name = "N")]
        version: u32,

        /// Preview changes without writing to file.
        #[arg(
            long,
            long_help = "Preview the mark operation without actually modifying the file.\n\
Useful for testing before applying changes."
        )]
        dry_run: bool,
    },

    /// Batch mark multiple text blocks from JSON input.
    #[command(
        long_about = "Insert anchor markers for multiple locations from JSON input.\n\
Designed for AI agents to efficiently mark many sections at once.\n\n\
JSON input format (array or object with 'marks' field):\n\
  [\n\
    {\"path\": \"README.md\", \"start_line\": 1, \"end_line\": 10, \"id\": \"intro\", \"tags\": [\"doc\"]},\n\
    {\"path\": \"src/main.rs\", \"start_line\": 5, \"end_line\": 20, \"id\": \"main\"}\n\
  ]\n\n\
Or:\n\
  {\"marks\": [{...}, {...}]}\n\n\
Marks in the same file are processed from bottom to top to avoid line shifts.\n\n\
Examples:\n\
  mise anchor batch --json '[{\"path\":\"a.md\",\"start_line\":1,\"end_line\":5,\"id\":\"a\"}]'\n\
  mise anchor batch --file marks.json\n\
  mise anchor batch --json '...' --dry-run\n"
    )]
    Batch {
        /// JSON string with mark specifications.
        #[arg(long, value_name = "JSON", conflicts_with = "file")]
        json: Option<String>,

        /// Path to JSON file with mark specifications.
        #[arg(long, value_name = "FILE", conflicts_with = "json")]
        file: Option<std::path::PathBuf>,

        /// Preview changes without writing to files.
        #[arg(long)]
        dry_run: bool,
    },

    /// Remove anchor markers from a file (unmark).
    #[command(
        long_about = "Remove anchor markers (begin and end tags) from a file.\n\
The content between the markers is preserved.\n\n\
Examples:\n\
  mise anchor unmark README.md --id intro\n\
  mise anchor unmark src/main.rs --id main.entry --dry-run\n"
    )]
    Unmark {
        /// File path to unmark (relative to ROOT).
        #[arg(value_name = "FILE")]
        file: String,

        /// Anchor ID to remove.
        #[arg(long, value_name = "ID")]
        id: String,

        /// Preview changes without writing to file.
        #[arg(long)]
        dry_run: bool,
    },
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

    /// Pack anchors and files into a context bundle for AI.
    #[command(
        long_about = "Bundle multiple anchors and files into a single context package.\n\
This is useful for preparing precise, controlled context for AI assistants.\n\n\
Features:\n\
- Combine multiple anchors and files\n\
- Token budget control with --max-tokens\n\
- Priority-based truncation (by confidence or order)\n\n\
Examples:\n\
  mise flow pack --anchors cli.scan,core.model\n\
  mise flow pack --anchors intro --files README.md Cargo.toml\n\
  mise flow pack --anchors api.handler --max-tokens 8000\n"
    )]
    Pack {
        /// Anchor IDs to include (comma-separated).
        #[arg(
            long,
            value_name = "IDS",
            value_delimiter = ',',
            long_help = "Comma-separated list of anchor IDs to include.\n\n\
Example: --anchors cli.scan,core.model,api.handler"
        )]
        anchors: Vec<String>,

        /// File paths to include.
        #[arg(
            long,
            value_name = "FILES",
            num_args = 0..,
            long_help = "File paths to include in the pack.\n\n\
Example: --files README.md src/main.rs"
        )]
        files: Vec<String>,

        /// Maximum tokens to include (estimated as chars/4).
        #[arg(
            long,
            value_name = "N",
            long_help = "Maximum number of tokens to include.\n\n\
Tokens are estimated as characters / 4. When over budget, items are\n\
truncated based on the priority mode."
        )]
        max_tokens: Option<usize>,

        /// Priority mode for truncation (confidence/order).
        #[arg(
            long,
            default_value = "confidence",
            value_name = "MODE",
            long_help = "Priority mode for truncation when over budget.\n\n\
Supported values:\n\
- confidence (default): keep high confidence items first\n\
- order: keep items in the order specified"
        )]
        priority: String,

        /// Show pack statistics on stderr.
        #[arg(
            long,
            long_help = "Print pack statistics (item count, token estimate) to stderr."
        )]
        stats: bool,
    },

    /// Calculate project statistics (word count, tokens, anchors).
    #[command(
        long_about = "Calculate comprehensive statistics for a writing project.\n\n\
Provides:\n\
- Total characters, words, and lines\n\
- CJK character count (for Chinese/Japanese/Korean)\n\
- Estimated token count (smart algorithm for mixed content)\n\
- Anchor statistics by tag\n\
- Top files by size\n\n\
Examples:\n\
  mise flow stats                           # Basic stats\n\
  mise flow stats --stats-format summary    # Human-readable summary\n\
  mise flow stats --stats-format json       # Full JSON output\n\
  mise flow stats --stats-format table      # Markdown table\n\
  mise flow stats --scope docs --exts md,txt\n\
  mise flow stats --top 20                  # Show top 20 files\n"
    )]
    Stats {
        /// Limit stats to a subdirectory.
        #[arg(
            long,
            value_name = "PATH",
            long_help = "Limit statistics to a specific subdirectory.\n\n\
Example: --scope docs"
        )]
        scope: Option<std::path::PathBuf>,

        /// File extensions to include (comma-separated).
        #[arg(
            long,
            value_name = "EXTS",
            value_delimiter = ',',
            long_help = "Filter files by extension.\n\n\
Default: md,txt,rst,adoc,org,tex,html,xml\n\
Example: --exts md,txt,rst"
        )]
        exts: Vec<String>,

        /// Output format (standard/json/summary/table).
        #[arg(
            long = "stats-format",
            value_name = "FORMAT",
            default_value = "summary",
            long_help = "Select the output format for statistics.\n\n\
Supported values:\n\
- summary (default): human-readable summary\n\
- json: full JSON object with all stats\n\
- table: Markdown table format\n\
- standard: ResultSet format (respects --format flag)"
        )]
        stats_format: String,

        /// Number of top files to show.
        #[arg(
            long,
            default_value = "10",
            value_name = "N",
            long_help = "Number of top files (by size) to include in the output."
        )]
        top: usize,
    },

    /// Generate document outline from anchors.
    #[command(
        long_about = "Generate a hierarchical outline of the project based on anchor structure.\n\n\
Shows:\n\
- All anchors organized by file\n\
- Character/word/token count per anchor\n\
- Content preview for each anchor\n\
- Nesting levels based on anchor structure\n\
- Tag-based grouping\n\n\
Examples:\n\
  mise flow outline                        # Full outline\n\
  mise flow outline --tag chapter          # Filter by tag\n\
  mise flow outline --outline-format tree  # Tree view\n\
  mise flow outline --outline-format json  # JSON output\n\
  mise flow outline --scope docs           # Limit to docs/\n"
    )]
    Outline {
        /// Limit outline to a subdirectory.
        #[arg(
            long,
            value_name = "PATH",
            long_help = "Limit outline to a specific subdirectory.\n\n\
Example: --scope docs"
        )]
        scope: Option<std::path::PathBuf>,

        /// Filter anchors by tag.
        #[arg(
            long,
            value_name = "TAG",
            long_help = "Only include anchors with this tag.\n\n\
Example: --tag chapter"
        )]
        tag: Option<String>,

        /// File extensions to include (comma-separated).
        #[arg(
            long,
            value_name = "EXTS",
            value_delimiter = ',',
            long_help = "Filter files by extension.\n\n\
Default: md,txt,rst,adoc,org,tex,html,xml\n\
Example: --exts md,txt"
        )]
        exts: Vec<String>,

        /// Output format (markdown/json/tree/standard).
        #[arg(
            long = "outline-format",
            value_name = "FORMAT",
            default_value = "markdown",
            long_help = "Select the output format for outline.\n\n\
Supported values:\n\
- markdown (default): Markdown document\n\
- json: full JSON object\n\
- tree: ASCII tree view\n\
- standard: ResultSet format"
        )]
        outline_format: String,
    },
}

/// Run the CLI with parsed arguments
pub fn run(cli: Cli) -> Result<()> {
    // Parse output format
    let format: OutputFormat = cli.format.parse().unwrap_or_default();
    let render_config = RenderConfig::with_pretty(format, cli.pretty);

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
            render_config,
        ),

        Commands::Find { pattern, scope } => crate::backends::scan::run_find(
            &root,
            pattern.as_deref(),
            scope.as_deref(),
            render_config,
        ),

        Commands::Extract {
            path,
            lines,
            max_bytes,
        } => crate::backends::extract::run_extract(&root, &path, &lines, max_bytes, render_config),

        Commands::Anchor { action } => match action {
            AnchorCommands::List { tag } => {
                crate::anchors::api::run_list(&root, tag.as_deref(), render_config)
            }
            AnchorCommands::Get { id, with_neighbors } => {
                crate::anchors::api::run_get(&root, &id, with_neighbors, render_config)
            }
            AnchorCommands::Lint => crate::anchors::lint::run_lint(&root, render_config),
            AnchorCommands::Mark {
                file,
                start,
                end,
                id,
                tags,
                version,
                dry_run,
            } => crate::anchors::mark::run_mark(
                &root,
                &file,
                start,
                end,
                &id,
                tags,
                version,
                dry_run,
                render_config,
            ),
            AnchorCommands::Batch {
                json,
                file,
                dry_run,
            } => {
                if let Some(json_str) = json {
                    crate::anchors::mark::run_batch_mark(&root, &json_str, dry_run, render_config)
                } else if let Some(file_path) = file {
                    crate::anchors::mark::run_batch_mark_from_file(
                        &root,
                        &file_path,
                        dry_run,
                        render_config,
                    )
                } else {
                    anyhow::bail!("Either --json or --file must be provided")
                }
            }
            AnchorCommands::Unmark { file, id, dry_run } => {
                crate::anchors::mark::run_unmark(&root, &file, &id, dry_run, render_config)
            }
        },

        Commands::Match { pattern, scope } => {
            crate::backends::rg::run_match(&root, &pattern, &scope, render_config)
        }

        Commands::Ast { pattern, scope } => {
            crate::backends::ast_grep::run_ast(&root, &pattern, &scope, render_config)
        }

        Commands::Deps {
            file,
            reverse,
            deps_format,
        } => {
            let deps_fmt: crate::backends::deps::DepsFormat =
                deps_format.parse().unwrap_or_default();
            crate::backends::deps::run_deps(
                &root,
                file.as_deref(),
                reverse,
                deps_fmt,
                render_config,
            )
        }

        Commands::Impact {
            staged,
            commit,
            diff,
            max_depth,
            impact_format,
        } => {
            let impact_fmt: crate::backends::impact::ImpactFormat =
                impact_format.parse().unwrap_or_default();
            crate::backends::impact::run_impact(
                &root,
                staged,
                commit.as_deref(),
                diff.as_deref(),
                max_depth,
                impact_fmt,
                render_config,
            )
        }

        Commands::Flow { action } => match action {
            FlowCommands::Writing { anchor, max_items } => {
                crate::flows::writing::run_writing(&root, &anchor, max_items, render_config)
            }
            FlowCommands::Pack {
                anchors,
                files,
                max_tokens,
                priority,
                stats,
            } => {
                let pack_priority: crate::flows::pack::PackPriority =
                    priority.parse().unwrap_or_default();
                crate::flows::pack::run_pack(
                    &root,
                    anchors,
                    files,
                    max_tokens,
                    pack_priority,
                    stats,
                    render_config,
                )
            }
            FlowCommands::Stats {
                scope,
                exts,
                stats_format,
                top,
            } => {
                let stats_fmt: crate::flows::stats::StatsFormat =
                    stats_format.parse().unwrap_or_default();
                let extensions = if exts.is_empty() { None } else { Some(exts) };
                crate::flows::stats::run_stats(
                    &root,
                    scope.as_deref(),
                    extensions,
                    stats_fmt,
                    top,
                    render_config,
                )
            }
            FlowCommands::Outline {
                scope,
                tag,
                exts,
                outline_format,
            } => {
                let outline_fmt: crate::flows::outline::OutlineFormat =
                    outline_format.parse().unwrap_or_default();
                let extensions = if exts.is_empty() { None } else { Some(exts) };
                crate::flows::outline::run_outline(
                    &root,
                    scope.as_deref(),
                    tag.as_deref(),
                    extensions,
                    outline_fmt,
                    render_config,
                )
            }
        },

        Commands::Rebuild => crate::cache::store::run_rebuild(&root, render_config),

        Commands::Doctor => crate::backends::doctor::run_doctor(render_config),

        #[cfg(feature = "watch")]
        Commands::Watch {
            cmd,
            exts,
            ignore,
            debounce,
            clear,
            restart,
            postpone,
        } => {
            let opts = crate::backends::watch::WatchOptions {
                cmd,
                extensions: exts,
                ignore,
                debounce,
                clear,
                restart,
                postpone,
                verbose: cli.verbose,
            };
            crate::backends::watch::run_watch(&root, opts, render_config)
        }
    }
}
