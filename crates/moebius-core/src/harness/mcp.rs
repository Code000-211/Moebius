//! The MCP servers of the harness configuration repository (`mcp.toml`, see
//! `docs/reference/harness-configuration.md`) and what a run is given for them: Pi's `mcp.json` or Claude Code's
//! `--mcp-config`, which name only the MCP gateway (see `gateway`); the gateway starts or connects to the servers of
//! the run's agent itself.
//!
//! A value of a server may take a value from the server's environment with `${NAME}`. Only the names the
//! administrator lists in `[harness] mcp_env` may be taken, so that a repository cannot pull other secrets of the
//! server, such as a provider's key, into a run. The names are checked when `mcp.toml` is read and the values are
//! looked up when a run's `mcp.json` is written, and handed to the gateway, not to the run.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use toml_edit::{Array, DocumentMut, InlineTable, Item, Table, value};

/// One server of `mcp.toml`: a stdio server (`command`) or a remote one (`url`). Built by `parse`, which checks it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct McpServer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
}

/// Reads `mcp.toml`: every server, or why it cannot be used. `Err` is why the file as a whole cannot be read.
pub fn parse_file(
    text: &str,
    allowed_env: &[String],
) -> Result<BTreeMap<String, Result<McpServer, String>>, String> {
    let table: toml::Table =
        toml::from_str(text).map_err(|e| format!("mcp.toml is not valid: {e}"))?;
    Ok(table
        .into_iter()
        .map(|(name, value)| {
            let server = parse(&name, value, allowed_env);
            (name, server)
        })
        .collect())
}

fn parse(name: &str, value: toml::Value, allowed_env: &[String]) -> Result<McpServer, String> {
    if !valid_name(name) {
        return Err("a server name is made of letters, digits, - and _".into());
    }
    let server: McpServer = value.try_into().map_err(|e| e.to_string())?;
    check(name, server, allowed_env)
}

/// `server` if it can be used under `name`, else why it cannot. Reading `mcp.toml` and editing it use this check.
pub fn check(name: &str, server: McpServer, allowed_env: &[String]) -> Result<McpServer, String> {
    if !valid_name(name) {
        return Err("a server name is made of letters, digits, - and _".into());
    }
    match (&server.command, &server.url) {
        (Some(command), None) if command.trim().is_empty() => {
            return Err("command must not be empty".into());
        }
        (Some(_), None) if !server.headers.is_empty() => {
            return Err("headers belong to a server with a url".into());
        }
        (None, Some(_)) if !server.args.is_empty() || !server.env.is_empty() => {
            return Err("args and env belong to a server with a command".into());
        }
        (Some(_), None) | (None, Some(_)) => {}
        _ => return Err("set either command or url".into()),
    }
    for value in server.values() {
        for variable in variables(value)? {
            if !allowed_env.contains(&variable) {
                return Err(format!(
                    "${{{variable}}} is not in mcp_env of [harness] in moebius.toml"
                ));
            }
        }
    }
    Ok(server)
}

/// Whether `name` can name a server.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// `text`, the content of `mcp.toml`, with the table `name` set to `server`: an existing table is rewritten in
/// place, a new one is added at the end. Other tables and comments are kept.
pub fn set_server(text: &str, name: &str, server: &McpServer) -> Result<String, String> {
    let mut doc: DocumentMut = text
        .parse()
        .map_err(|e| format!("mcp.toml is not valid: {e}"))?;
    let strings = |values: &BTreeMap<String, String>| {
        let mut table = InlineTable::new();
        for (key, text) in values {
            table.insert(key, text.as_str().into());
        }
        value(table)
    };
    let mut fresh = Table::new();
    if let Some(command) = &server.command {
        fresh["command"] = value(command.as_str());
    }
    if !server.args.is_empty() {
        fresh["args"] = value(server.args.iter().map(String::as_str).collect::<Array>());
    }
    if !server.env.is_empty() {
        fresh["env"] = strings(&server.env);
    }
    if let Some(url) = &server.url {
        fresh["url"] = value(url.as_str());
    }
    if !server.headers.is_empty() {
        fresh["headers"] = strings(&server.headers);
    }
    match doc.get_mut(name).and_then(Item::as_table_mut) {
        Some(table) => {
            table.clear();
            for (key, item) in fresh {
                table.insert(&key, item);
            }
        }
        None => {
            if !doc.as_table().is_empty() {
                fresh.decor_mut().set_prefix("\n");
            }
            doc.insert(name, Item::Table(fresh));
        }
    }
    Ok(doc.to_string())
}

/// `text`, the content of `mcp.toml`, without the table `name`.
pub fn remove_server(text: &str, name: &str) -> Result<String, String> {
    let mut doc: DocumentMut = text
        .parse()
        .map_err(|e| format!("mcp.toml is not valid: {e}"))?;
    doc.remove(name);
    Ok(doc.to_string())
}

impl McpServer {
    /// `stdio` for a server with a command, `remote` for one with a url.
    pub fn kind(&self) -> &'static str {
        if self.command.is_some() {
            "stdio"
        } else {
            "remote"
        }
    }

    fn values(&self) -> impl Iterator<Item = &String> {
        self.command
            .iter()
            .chain(&self.args)
            .chain(self.env.values())
            .chain(&self.url)
            .chain(self.headers.values())
    }

    /// The server with every `${NAME}` replaced by what `lookup` gives, as the gateway starts or connects to it.
    fn with_values(&self, lookup: &impl Fn(&str) -> Option<String>) -> Result<McpServer, String> {
        let fill = |value: &String| fill(value, lookup);
        let fill_all = |values: &BTreeMap<String, String>| {
            values
                .iter()
                .map(|(key, value)| Ok((key.clone(), fill(value)?)))
                .collect::<Result<BTreeMap<_, _>, String>>()
        };
        Ok(McpServer {
            command: self.command.as_ref().map(fill).transpose()?,
            args: self.args.iter().map(fill).collect::<Result<_, _>>()?,
            env: fill_all(&self.env)?,
            url: self.url.as_ref().map(fill).transpose()?,
            headers: fill_all(&self.headers)?,
        })
    }
}

/// `servers` with the values of `${NAME}` taken from `lookup`; a name `lookup` does not know is an error, since the run
/// would be given a server that cannot work.
pub fn with_values(
    servers: &BTreeMap<String, McpServer>,
    lookup: &impl Fn(&str) -> Option<String>,
) -> Result<BTreeMap<String, McpServer>, String> {
    servers
        .iter()
        .map(|(name, server)| {
            let filled = server
                .with_values(lookup)
                .map_err(|e| format!("the MCP server {name:?}: {e}"))?;
            Ok((name.clone(), filled))
        })
        .collect()
}

/// Pi's `mcp.json` for a run whose agent names servers: the MCP gateway at `url` as its only server, named
/// [`GATEWAY`], with the run's token. Its tools are declared to the model directly, so that they pass through
/// `tool_call` like Pi's own.
pub fn pi_config(url: &str, token: &str) -> Value {
    json!({ "mcpServers": { GATEWAY: {
        "url": url,
        "headers": { "Authorization": format!("Bearer {token}") },
        "exposure": "direct",
    } } })
}

/// The name of the MCP gateway in a run's `mcp.json`.
pub const GATEWAY: &str = "moebius";

/// Claude Code's `--mcp-config` for a run whose agent names servers: as [`pi_config`], the MCP gateway at `url` as its
/// only server, with the run's token, in Claude Code's shape.
pub fn claude_config(url: &str, token: &str) -> Value {
    json!({ "mcpServers": { GATEWAY: {
        "type": "http",
        "url": url,
        "headers": { "Authorization": format!("Bearer {token}") },
    } } })
}

/// The names `${NAME}` in `text`. A `${` that does not close over a name is an error.
fn variables(text: &str) -> Result<Vec<String>, String> {
    let mut names = vec![];
    let mut rest = text;
    while let Some(start) = rest.find("${") {
        let after = &rest[start + 2..];
        let end = after
            .find('}')
            .ok_or_else(|| format!("{text:?} has a ${{ that is not closed"))?;
        let name = &after[..end];
        if !valid_variable(name) {
            return Err(format!("{name:?} is not a name for ${{NAME}}"));
        }
        names.push(name.to_string());
        rest = &after[end + 1..];
    }
    Ok(names)
}

/// Whether `name` can be listed in `mcp_env` and used as `${NAME}`.
pub fn valid_variable(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with(|c: char| c.is_ascii_digit())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn fill(text: &str, lookup: &impl Fn(&str) -> Option<String>) -> Result<String, String> {
    let mut filled = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("${") {
        let after = &rest[start + 2..];
        let end = after.find('}').ok_or("a ${ is not closed")?;
        let name = &after[..end];
        let value = lookup(name).ok_or_else(|| format!("${{{name}}} is not set"))?;
        filled.push_str(&rest[..start]);
        filled.push_str(&value);
        rest = &after[end + 1..];
    }
    filled.push_str(rest);
    Ok(filled)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    fn server(text: &str, allowed: &[&str]) -> Result<McpServer, String> {
        let mut servers = parse_file(&format!("[s]\n{text}"), &names(allowed)).unwrap();
        servers.remove("s").unwrap()
    }

    #[test]
    fn a_stdio_server_and_a_remote_one_are_read() {
        let stdio = server(
            "command = \"docs\"\nargs = [\"--root\", \"/srv\"]\nenv = { KEY = \"${DOCS_KEY}\" }\n",
            &["DOCS_KEY"],
        )
        .unwrap();
        assert_eq!(stdio.command.as_deref(), Some("docs"));
        assert_eq!(stdio.args, ["--root", "/srv"]);
        let remote = server(
            "url = \"https://x.test/mcp\"\nheaders = { Authorization = \"Bearer ${T}\" }\n",
            &["T"],
        )
        .unwrap();
        assert_eq!(remote.url.as_deref(), Some("https://x.test/mcp"));
    }

    #[test]
    fn a_server_that_is_wrong_is_an_error_for_that_server_only() {
        let servers = parse_file(
            "[good]\ncommand = \"a\"\n\
             [typo]\ncomand = \"a\"\n\
             [both]\ncommand = \"a\"\nurl = \"u\"\n\
             [neither]\n\
             [mixed]\nurl = \"u\"\nargs = [\"x\"]\n\
             [bad.name]\ncommand = \"a\"\n\
             [secret]\ncommand = \"a\"\nenv = { K = \"${OPENAI_API_KEY}\" }\n\
             [open]\ncommand = \"${A\"\n",
            &names(&["DOCS_KEY"]),
        )
        .unwrap();
        assert!(servers["good"].is_ok());
        assert!(servers["typo"].as_ref().unwrap_err().contains("comand"));
        for broken in ["both", "neither", "mixed", "bad", "open"] {
            assert!(servers[broken].is_err(), "{broken}");
        }
        assert!(
            servers["secret"]
                .as_ref()
                .unwrap_err()
                .contains("${OPENAI_API_KEY} is not in mcp_env")
        );
        assert!(parse_file("[x", &[]).is_err());
    }

    #[test]
    fn a_table_is_rewritten_in_place_and_the_rest_of_the_file_is_kept() {
        let text = "# The servers.\n\n# Docs.\n[docs]\ncommand = \"old\"\n\n# Search.\n[search]\nurl = \"https://x.test\"\n";
        let server = McpServer {
            command: Some("new".into()),
            args: vec!["--root".into(), "/srv".into()],
            env: BTreeMap::from([("KEY".to_string(), "${DOCS_KEY}".to_string())]),
            url: None,
            headers: BTreeMap::new(),
        };
        let changed = set_server(text, "docs", &server).unwrap();
        assert!(
            changed.starts_with("# The servers.\n\n# Docs.\n[docs]\n"),
            "{changed}"
        );
        assert!(
            changed.contains("# Search.\n[search]\nurl = \"https://x.test\"\n"),
            "{changed}"
        );
        let read = parse_file(&changed, &names(&["DOCS_KEY"])).unwrap();
        assert_eq!(read["docs"].as_ref().unwrap(), &server);
        assert!(read["search"].is_ok());

        let added = set_server(&changed, "fresh", &server).unwrap();
        assert!(
            added.starts_with(&changed) && added.contains("\n[fresh]\ncommand = \"new\""),
            "{added}"
        );
        assert_eq!(parse_file(&added, &names(&["DOCS_KEY"])).unwrap().len(), 3);
        assert!(
            set_server("", "first", &server)
                .unwrap()
                .starts_with("[first]")
        );

        let removed = remove_server(&added, "docs").unwrap();
        assert!(
            !removed.contains("[docs]")
                && removed.contains("# Search.")
                && removed.contains("[search]")
        );
        assert!(set_server("[x", "a", &server).is_err());
    }

    #[test]
    fn a_server_is_checked_as_reading_checks_it() {
        let both = McpServer {
            command: Some("a".into()),
            args: vec![],
            env: BTreeMap::new(),
            url: Some("u".into()),
            headers: BTreeMap::new(),
        };
        assert!(check("s", both.clone(), &[]).is_err());
        let secret = McpServer {
            url: None,
            env: BTreeMap::from([("K".to_string(), "${OPENAI_API_KEY}".to_string())]),
            ..both
        };
        assert!(
            check("s", secret.clone(), &[])
                .unwrap_err()
                .contains("is not in mcp_env")
        );
        assert!(check("bad name", secret, &names(&["OPENAI_API_KEY"])).is_err());
    }

    #[test]
    fn the_gateway_is_given_the_values_not_the_names() {
        let docs = server(
            "command = \"docs-${V}\"\nargs = [\"${V}\"]\nenv = { KEY = \"k-${V}-${W}\" }\n",
            &["V", "W"],
        )
        .unwrap();
        let lookup = |name: &str| match name {
            "V" => Some("1".to_string()),
            "W" => Some("2".to_string()),
            _ => None,
        };
        let servers = BTreeMap::from([("docs".to_string(), docs.clone())]);
        let filled = with_values(&servers, &lookup).unwrap();
        assert_eq!(filled["docs"].command.as_deref(), Some("docs-1"));
        assert_eq!(filled["docs"].args, ["1"]);
        assert_eq!(filled["docs"].env["KEY"], "k-1-2");
        let unset = with_values(&servers, &|_| None);
        assert!(unset.unwrap_err().contains("${V} is not set"));

        assert_eq!(
            pi_config("http://127.0.0.1:9/mcp", "moebius-run-x"),
            json!({ "mcpServers": { "moebius": {
                "url": "http://127.0.0.1:9/mcp",
                "headers": { "Authorization": "Bearer moebius-run-x" },
                "exposure": "direct"
            } } })
        );
    }
}
