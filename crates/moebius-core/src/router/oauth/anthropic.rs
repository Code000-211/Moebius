//! What an Anthropic subscription token (`sk-ant-oat...`, the `anthropic` preset in `preset`) needs on every
//! `POST /v1/messages` request so that Anthropic bills the request to the person's Claude Pro or Max subscription
//! rather than rejecting it as extra usage (`You're out of extra usage.`) or as an unsupported client version:
//!
//! 1. The `x-anthropic-billing-header` text block as the first block of `system` whenever `messages` has a user text,
//!    with no `cache_control`, followed by [`CLAUDE_CODE_PREFIX`].
//! 2. Section-aware de-fingerprinting of Pi and Pi-Bolt system prompts and mid-conversation `role: "system"` section
//!    updates, keeping `<addendum>`, `<project_context>`, `<skills>`, `<cwd>` and custom prompts intact.
//! 3. A Claude Code version floor in `user-agent` (`claude-cli/<version>`) and `cc_version`, raised when a client
//!    sends a newer `claude-cli/<version>` header or when the upstream refuses with `claude_code_version_too_old`.

use axum::body::Bytes;
use axum::http::{HeaderMap, StatusCode, header};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::Logins;

/// What the system prompt of a request with an Anthropic subscription token must carry after any billing header block.
pub(in crate::router) const CLAUDE_CODE_PREFIX: &str =
    "You are Claude Code, Anthropic's official CLI for Claude.";

/// The default Claude Code version floor sent in `user-agent` and `x-anthropic-billing-header`.
pub(in crate::router) const DEFAULT_CLAUDE_CODE_VERSION: &str = "2.1.280";

/// Salt and UTF-16 sample positions for the 3-character version suffix in `x-anthropic-billing-header`.
const BILLING_HEADER_SALT: &str = "59cf53e54c78";
const BILLING_HEADER_INDICES: [usize; 3] = [4, 7, 20];
const BILLING_HEADER_KEY: &str = "x-anthropic-billing-header:";

/// Neutral preamble that replaces Pi's and Pi-Bolt's default harness preamble on Anthropic subscription requests.
pub(super) const MINIMAL_ANTHROPIC_OAUTH_PROMPT: &str = "You are an expert coding assistant.\n\
Be concise and helpful.\n\
Use the available tools to answer the user's request.\n\
Show file paths clearly when working with files.";

/// Prefix shared by Pi's (`operating inside pi, a coding agent harness`) and Pi-Bolt's
/// (`operating inside Pi-Bolt, a build of pi`) default preambles.
const PI_PREAMBLE_NEEDLE: &str = "You are an expert coding assistant operating inside ";
const PI_DOCS_NEEDLE: &str = "Pi documentation (read only when the user asks about pi itself";
const PI_TOOLS_FILLER_NEEDLE: &str = "In addition to the tools above";

const TEXT_REPLACEMENTS: &[(&str, &str)] = &[(
    "Here is some useful information about the environment you are running in:",
    "Environment context you are running in:",
)];

/// Parses a strict `major.minor.patch` version into its three numbers.
pub(in crate::router) fn parse_version(version: &str) -> Option<(u64, u64, u64)> {
    let mut parts = version.trim().split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

/// Whether `candidate` is a valid `major.minor.patch` version newer than `current`.
pub(in crate::router) fn is_newer_version(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(cand), Some(curr)) => cand > curr,
        _ => false,
    }
}

/// Extracts `X.Y.Z` from a `user-agent: claude-cli/X.Y.Z...` header, if present and valid.
fn version_from_user_agent(headers: &HeaderMap) -> Option<String> {
    let ua = headers.get(header::USER_AGENT)?.to_str().ok()?;
    let rest = ua.strip_prefix("claude-cli/")?;
    let raw = rest
        .split(|c: char| c.is_whitespace() || c == '(' || c == ';')
        .next()?;
    parse_version(raw).map(|_| raw.to_string())
}

impl Logins {
    /// The Claude Code version to send for an Anthropic preset request: the highest of [`DEFAULT_CLAUDE_CODE_VERSION`],
    /// any version learned from an upstream rejection, and any `claude-cli/<version>` sent by the harness in `headers`.
    pub(in crate::router) fn claude_code_version(&self, headers: &HeaderMap) -> String {
        let mut floor = self
            .claude_code_version
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let current = floor.as_deref().unwrap_or(DEFAULT_CLAUDE_CODE_VERSION);
        if let Some(client_version) = version_from_user_agent(headers)
            && is_newer_version(&client_version, current)
        {
            *floor = Some(client_version.clone());
            return client_version;
        }
        current.to_string()
    }

    /// Raises the remembered Claude Code version floor when `version` is newer.
    pub(in crate::router) fn note_claude_code_version(&self, version: &str) -> bool {
        let mut floor = self
            .claude_code_version
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let current = floor.as_deref().unwrap_or(DEFAULT_CLAUDE_CODE_VERSION);
        if is_newer_version(version, current) {
            *floor = Some(version.to_string());
            true
        } else {
            false
        }
    }
}

/// Builds `x-anthropic-billing-header: cc_version=<version>.<suffix>; cc_entrypoint=sdk-cli; cch=<cch>;` from the
/// first user message's text and the Claude Code version.
pub(in crate::router) fn billing_header(first_user_text: &str, version: &str) -> String {
    let cch_full = hex::encode(Sha256::digest(first_user_text.as_bytes()));
    let cch = &cch_full[..5];

    let utf16: Vec<u16> = first_user_text.encode_utf16().collect();
    let mut sampled = String::with_capacity(BILLING_HEADER_INDICES.len());
    for index in BILLING_HEADER_INDICES {
        let ch = utf16
            .get(index)
            .copied()
            .filter(|&unit| unit != 0)
            .and_then(|unit| char::from_u32(u32::from(unit)))
            .unwrap_or('0');
        sampled.push(ch);
    }

    let suffix_input = format!("{BILLING_HEADER_SALT}{sampled}{version}");
    let suffix_full = hex::encode(Sha256::digest(suffix_input.as_bytes()));
    let suffix = &suffix_full[..3];

    format!(
        "x-anthropic-billing-header: cc_version={version}.{suffix}; cc_entrypoint=sdk-cli; cch={cch};"
    )
}

/// Extracts the text of the first `role: "user"` message in `request["messages"]`, if it is non-empty.
fn first_user_text(request: &Value) -> Option<String> {
    let messages = request.get("messages")?.as_array()?;
    for message in messages {
        if message.get("role").and_then(Value::as_str) != Some("user") {
            continue;
        }
        match message.get("content") {
            Some(Value::String(text)) => {
                return (!text.is_empty()).then(|| text.clone());
            }
            Some(Value::Array(blocks)) => {
                for block in blocks {
                    if block.get("type").and_then(Value::as_str) == Some("text")
                        && let Some(text) = block.get("text").and_then(Value::as_str)
                    {
                        return (!text.is_empty()).then(|| text.to_string());
                    }
                }
                return None;
            }
            _ => return None,
        }
    }
    None
}

fn apply_text_replacements(text: &str) -> String {
    let mut out = text.to_string();
    for (needle, replacement) in TEXT_REPLACEMENTS {
        if out.contains(needle) {
            out = out.replace(needle, replacement);
        }
    }
    out
}

fn strip_tools_filler(body: &str) -> String {
    body.split("\n\n")
        .filter(|paragraph| !paragraph.contains(PI_TOOLS_FILLER_NEEDLE))
        .collect::<Vec<_>>()
        .join("\n\n")
}

enum PromptChunk<'a> {
    Untagged {
        name: &'a str,
        raw: &'a str,
    },
    Tagged {
        name: &'a str,
        body: &'a str,
        raw: &'a str,
    },
}

fn is_valid_tag_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_alphabetic() && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Parses a structured Pi or Pi-Bolt system prompt into its untagged preamble and `<tag>\n...\n</tag>` sections.
fn parse_pi_system_prompt(text: &str) -> Option<Vec<PromptChunk<'_>>> {
    if !text.contains(PI_PREAMBLE_NEEDLE) {
        return None;
    }
    let mut chunks = Vec::new();
    let mut pos = 0;
    let mut scan = 0;
    let bytes = text.as_bytes();

    while scan < bytes.len() {
        let Some(rel_lt) = text[scan..].find('<') else {
            break;
        };
        let tag_start = scan + rel_lt;
        let after_lt = &text[tag_start + 1..];
        let Some(rel_gt) = after_lt.find(">\n") else {
            scan = tag_start + 1;
            continue;
        };
        let tag_name = &after_lt[..rel_gt];
        if !is_valid_tag_name(tag_name) {
            scan = tag_start + 1;
            continue;
        }
        let body_start = tag_start + 1 + rel_gt + 2;
        let close_marker = format!("\n</{tag_name}>");
        let Some(rel_close) = text[body_start..].find(&close_marker) else {
            scan = tag_start + 1;
            continue;
        };
        let body_end = body_start + rel_close;
        let tag_end = body_end + close_marker.len();

        let gap = &text[pos..tag_start];
        if !gap.trim().is_empty() {
            let name = if chunks.is_empty() {
                "preamble"
            } else {
                "untagged"
            };
            chunks.push(PromptChunk::Untagged {
                name,
                raw: gap.trim_matches('\n'),
            });
        }
        chunks.push(PromptChunk::Tagged {
            name: tag_name,
            body: &text[body_start..body_end],
            raw: &text[tag_start..tag_end],
        });
        pos = tag_end;
        scan = tag_end;
    }

    if pos < text.len() {
        let tail = &text[pos..];
        if !tail.trim().is_empty() {
            let name = if chunks.is_empty() {
                "preamble"
            } else {
                "untagged"
            };
            chunks.push(PromptChunk::Untagged {
                name,
                raw: tail.trim_matches('\n'),
            });
        }
    }

    let has_pi_section = chunks.iter().any(|chunk| {
        matches!(
            chunk,
            PromptChunk::Tagged {
                name: "tools" | "rules" | "docs",
                ..
            }
        )
    });
    has_pi_section.then_some(chunks)
}

/// De-fingerprints a Pi or Pi-Bolt system prompt while keeping project context, skills, addendum, cwd and custom
/// prompts intact.
pub(in crate::router) fn sanitize_system_prompt(text: &str) -> String {
    let Some(chunks) = parse_pi_system_prompt(text) else {
        return text.to_string();
    };
    let mut kept = Vec::with_capacity(chunks.len());
    for chunk in chunks {
        match chunk {
            PromptChunk::Untagged {
                name: "preamble", ..
            } => {
                kept.push(apply_text_replacements(MINIMAL_ANTHROPIC_OAUTH_PROMPT));
            }
            PromptChunk::Tagged {
                name: "docs",
                body,
                raw,
            } => {
                if body.contains(PI_DOCS_NEEDLE) {
                    continue;
                }
                kept.push(apply_text_replacements(raw));
            }
            PromptChunk::Tagged {
                name: "tools",
                body,
                ..
            } => {
                let cleaned = apply_text_replacements(&strip_tools_filler(body));
                kept.push(format!("<tools>\n{cleaned}\n</tools>"));
            }
            PromptChunk::Tagged {
                name: "rules", raw, ..
            } => {
                kept.push(apply_text_replacements(raw));
            }
            PromptChunk::Untagged { raw, .. } | PromptChunk::Tagged { raw, .. } => {
                kept.push(raw.to_string());
            }
        }
    }
    kept.join("\n\n")
}

/// Sanitizes a mid-conversation `role: "system"` section update block (`Updated system prompt section "<name>":\n\n...`
/// or `Removed system prompt section "<name>".`). Returns `None` when the block is a Pi `<docs>` update or removal that
/// should be dropped.
fn sanitize_section_update_text(text: &str) -> Option<String> {
    if let Some(rest) = text.strip_prefix("Removed system prompt section \"")
        && let Some(name) = rest.strip_suffix("\".")
    {
        if name == "docs" {
            return None;
        }
        return Some(text.to_string());
    }

    if let Some(rest) = text.strip_prefix("Updated system prompt section \"")
        && let Some((name, body)) = rest.split_once("\":\n\n")
    {
        return match name {
            "preamble" => {
                if !body.contains(PI_PREAMBLE_NEEDLE) {
                    return Some(text.to_string());
                }
                let replaced = apply_text_replacements(MINIMAL_ANTHROPIC_OAUTH_PROMPT);
                Some(format!(
                    "Updated system prompt section \"preamble\":\n\n{replaced}"
                ))
            }
            "docs" => {
                let inner = body
                    .strip_prefix("<docs>\n")
                    .and_then(|s| s.strip_suffix("\n</docs>"))
                    .unwrap_or(body);
                if inner.contains(PI_DOCS_NEEDLE) {
                    None
                } else {
                    let replaced = apply_text_replacements(body);
                    Some(format!(
                        "Updated system prompt section \"docs\":\n\n{replaced}"
                    ))
                }
            }
            "tools" => {
                let updated = if let Some(inner) = body
                    .strip_prefix("<tools>\n")
                    .and_then(|s| s.strip_suffix("\n</tools>"))
                {
                    let cleaned = apply_text_replacements(&strip_tools_filler(inner));
                    format!("<tools>\n{cleaned}\n</tools>")
                } else {
                    apply_text_replacements(&strip_tools_filler(body))
                };
                Some(format!(
                    "Updated system prompt section \"tools\":\n\n{updated}"
                ))
            }
            "rules" => {
                let replaced = apply_text_replacements(body);
                Some(format!(
                    "Updated system prompt section \"rules\":\n\n{replaced}"
                ))
            }
            _ => Some(text.to_string()),
        };
    }

    Some(sanitize_system_prompt(text))
}

/// Sanitizes any `role: "system"` messages inside `request["messages"]`.
fn sanitize_system_messages(request: &mut Value) {
    let Some(messages) = request.get_mut("messages").and_then(Value::as_array_mut) else {
        return;
    };
    let mut kept_messages = Vec::with_capacity(messages.len());
    for mut message in std::mem::take(messages) {
        if message.get("role").and_then(Value::as_str) != Some("system") {
            kept_messages.push(message);
            continue;
        }
        let has_output_config = message.get("output_config").is_some();
        match message.get_mut("content") {
            Some(Value::String(text)) => match sanitize_section_update_text(text) {
                Some(updated) => {
                    *text = updated;
                    kept_messages.push(message);
                }
                None if has_output_config => {
                    *text = String::new();
                    kept_messages.push(message);
                }
                None => {}
            },
            Some(Value::Array(blocks)) => {
                let mut kept_blocks = Vec::with_capacity(blocks.len());
                for mut block in std::mem::take(blocks) {
                    if block.get("type").and_then(Value::as_str) == Some("text")
                        && let Some(text) = block.get("text").and_then(Value::as_str)
                    {
                        if let Some(updated) = sanitize_section_update_text(text) {
                            block["text"] = Value::String(updated);
                            kept_blocks.push(block);
                        }
                    } else {
                        kept_blocks.push(block);
                    }
                }
                *blocks = kept_blocks;
                if !blocks.is_empty() || has_output_config {
                    kept_messages.push(message);
                }
            }
            _ => kept_messages.push(message),
        }
    }
    *messages = kept_messages;
}

/// Shapes a `POST /v1/messages` JSON body for an Anthropic subscription request using `version`.
pub(in crate::router) fn shape_request(request: &mut Value, version: &str) {
    sanitize_system_messages(request);

    let mut blocks = match request.get_mut("system").map(Value::take) {
        Some(Value::String(text)) if !text.is_empty() => {
            vec![json!({"type": "text", "text": text})]
        }
        Some(Value::Array(blocks)) => blocks,
        _ => Vec::new(),
    };

    for block in &mut blocks {
        if let Some(text) = block.get("text").and_then(Value::as_str) {
            let sanitized = sanitize_system_prompt(text);
            if sanitized != text {
                block["text"] = Value::String(sanitized);
            }
        }
    }

    let has_billing_header = blocks.iter().any(|block| {
        block
            .get("text")
            .and_then(Value::as_str)
            .is_some_and(|text| text.contains(BILLING_HEADER_KEY))
    });
    let prefix_block = json!({"type": "text", "text": CLAUDE_CODE_PREFIX});
    let has_prefix = blocks.iter().any(|block| block == &prefix_block);

    if !has_prefix {
        let insert_at = usize::from(
            blocks
                .first()
                .and_then(|b| b.get("text"))
                .and_then(Value::as_str)
                .is_some_and(|text| text.contains(BILLING_HEADER_KEY)),
        );
        blocks.insert(insert_at, prefix_block);
    }

    if let Some(user_text) = first_user_text(request)
        && !has_billing_header
    {
        blocks.insert(
            0,
            json!({
                "type": "text",
                "text": billing_header(&user_text, version),
            }),
        );
    }

    request["system"] = Value::Array(blocks);
}

/// If `bytes` is an Anthropic `claude_code_version_too_old` error body asking for a `major.minor.patch` version newer
/// than `sent_version`, returns that required version.
pub(in crate::router) fn required_version_from_error(
    bytes: &[u8],
    sent_version: &str,
) -> Option<String> {
    let body: Value = serde_json::from_slice(bytes).ok()?;
    let code = body
        .get("error")?
        .get("details")?
        .get("error_code")?
        .as_str()?;
    if code != "claude_code_version_too_old" {
        return None;
    }
    let message = body.get("error")?.get("message")?.as_str()?;
    let lower = message.to_ascii_lowercase();
    let (before_newer, _) = lower.split_once(" or newer is required")?;
    let (_, required) = before_newer.rsplit_once("version ")?;
    let required = required.trim();
    is_newer_version(required, sent_version).then(|| required.to_string())
}

/// Rebuilds a [`reqwest::Response`] from status, headers and body bytes read when inspecting a 400 error.
pub(in crate::router) fn rebuild_response(
    status: StatusCode,
    headers: HeaderMap,
    bytes: Bytes,
) -> reqwest::Response {
    let mut builder = axum::http::Response::builder().status(status);
    if let Some(map) = builder.headers_mut() {
        *map = headers;
    }
    reqwest::Response::from(builder.body(bytes).unwrap())
}
