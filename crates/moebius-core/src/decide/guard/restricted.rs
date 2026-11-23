//! The restricted mode of a review run of a pull request someone else opened (ADR 0029). What such a pull request holds
//! is written by a person the server does not trust, so its review run may only read files of its worktree: the tools
//! that read and list files, and nothing else. No shell, so nothing of the pull request runs on this machine (not its
//! build scripts, its tests or a hook), no network, no write, and no tool of an MCP server. Fixed like the built-in
//! patterns, whatever the rules in prose say.

use std::path::Path;

use serde_json::Value;

use super::shell::{ToolCall, normalise_call};

/// The name the rule is refused under, and the value of `restricted` in the question's context.
pub const RESTRICTED: &str = "restricted-review";

/// What the rule says, as it is given as the reason of a refusal.
const PROSE: &str = "a review of a pull request someone else opened only reads files of its worktree: no shell, no network, no writes and no MCP tools";

/// The tools of a restricted run: Pi's tools that read and list files and Claude Code's.
const READ_TOOLS: [&str; 8] = ["read", "grep", "find", "ls", "Read", "Grep", "Glob", "LS"];

/// The reason `tool` may not be called by a restricted run with `input`, none when it may. A file path it names must be
/// inside `worktree`; a run without a worktree reads nothing.
pub fn restricted_refusal(tool: &str, input: &Value, worktree: Option<&Path>) -> Option<String> {
    let (tool, input) = normalise_call(tool, input.clone());
    if !READ_TOOLS.contains(&tool.as_str()) {
        return Some(format!("{RESTRICTED}: {tool} is refused: {PROSE}"));
    }
    let call = ToolCall {
        tool: &tool,
        input,
        worktree,
        scratch: None,
        others: &[],
    };
    let named = ["path", "file_path", "notebook_path"]
        .iter()
        .filter_map(|key| call.input[*key].as_str());
    for path in named {
        if call.outside_worktree(&call.start_dir(), path) {
            return Some(format!(
                "{RESTRICTED}: {path} is outside the worktree: {PROSE}"
            ));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn only_the_tools_that_read_files_of_the_worktree_are_allowed() {
        let tree = Path::new("/srv/wt/9");
        for (tool, input) in [
            ("read", json!({"path": "src/main.rs"})),
            ("read", json!({"path": "/srv/wt/9/src/main.rs"})),
            ("grep", json!({"pattern": "todo"})),
            ("ls", json!({})),
            ("Read", json!({"file_path": "/srv/wt/9/a.txt"})),
            ("Glob", json!({"pattern": "**/*.rs"})),
        ] {
            assert_eq!(restricted_refusal(tool, &input, Some(tree)), None, "{tool}");
        }
    }

    #[test]
    fn a_shell_the_network_writes_and_mcp_tools_are_refused() {
        let tree = Path::new("/srv/wt/9");
        for (tool, input) in [
            ("bash", json!({"command": "ls"})),
            ("bash", json!({"command": "cargo test"})),
            ("Bash", json!({"command": "curl https://example.com"})),
            ("write", json!({"path": "a.txt", "content": "x"})),
            ("edit", json!({"path": "a.txt"})),
            ("WebFetch", json!({"url": "https://example.com"})),
            ("WebSearch", json!({"query": "x"})),
            ("fetch", json!({"url": "https://example.com"})),
            ("search__query", json!({"q": "x"})),
        ] {
            let refusal = restricted_refusal(tool, &input, Some(tree))
                .unwrap_or_else(|| panic!("{tool} was allowed"));
            assert!(refusal.starts_with(RESTRICTED), "{refusal}");
        }
    }

    #[test]
    fn a_read_outside_the_worktree_or_without_one_is_refused() {
        let tree = Path::new("/srv/wt/9");
        for path in [
            "/etc/passwd",
            "../other/a.txt",
            "~/.ssh/id_rsa",
            "/srv/wt/90/a",
        ] {
            assert!(
                restricted_refusal("read", &json!({ "path": path }), Some(tree)).is_some(),
                "{path}"
            );
        }
        assert!(restricted_refusal("read", &json!({"path": "a.txt"}), None).is_some());
    }
}
