//! A run's events folded into what the live view shows: the agent's text, its tool calls with each check's decision, the
//! person's messages and the errors, in order. No iced here: events go in one by one (the stream) or as a page, in any
//! order, and an event seen twice counts once, so both ways give the same conversation.

use std::collections::{BTreeMap, HashMap};

use serde_json::Value;

use crate::client::{Judgement, RunEvent};

/// What a tool call does, shown compactly.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Input {
    /// The input is not known, or is none the live view knows how to show.
    #[default]
    Unknown,
    /// `read`: the path.
    Read(String),
    /// `edit`: the path and the replacements, old and new text.
    Edit {
        path: String,
        changes: Vec<(String, String)>,
    },
    /// `write`: the path and the new content.
    Write { path: String, content: String },
    /// `bash`: the command.
    Command(String),
    /// Any other tool: its input as JSON.
    Other(String),
}

impl Input {
    /// The input in one line, as the head of a tool call shows it.
    pub fn summary(&self) -> String {
        match self {
            Input::Unknown => String::new(),
            Input::Read(path) | Input::Edit { path, .. } | Input::Write { path, .. } => {
                path.clone()
            }
            Input::Command(command) => command.lines().next().unwrap_or_default().to_string(),
            Input::Other(json) => json.clone(),
        }
    }

    fn of(tool: &str, args: &Value) -> Input {
        let text = |key: &str| args[key].as_str().unwrap_or_default().to_string();
        match tool {
            "read" => Input::Read(text("path")),
            "write" => Input::Write {
                path: text("path"),
                content: text("content"),
            },
            "bash" => Input::Command(text("command")),
            "edit" => {
                // Pi's edit takes `edits`, and still the single `oldText` and `newText` of its older form.
                let pair = |edit: &Value| {
                    (
                        edit["oldText"].as_str().unwrap_or_default().to_string(),
                        edit["newText"].as_str().unwrap_or_default().to_string(),
                    )
                };
                let mut changes: Vec<_> = args["edits"]
                    .as_array()
                    .map(|edits| edits.iter().map(pair).collect())
                    .unwrap_or_default();
                if args["oldText"].is_string() {
                    changes.push(pair(args));
                }
                Input::Edit {
                    path: text("path"),
                    changes,
                }
            }
            _ if args.is_null() => Input::Unknown,
            _ => Input::Other(args.to_string()),
        }
    }
}

/// The answer of the tool-call check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub allowed: bool,
    pub reason: String,
    /// Who decided (`patterns`, `teacher`, ...), when the judgements say.
    pub decider: Option<String>,
    /// What happened when the refused call was run again in a copy (`exit 0; deleted 3 files`), when the judgements
    /// say.
    pub replay: Option<Box<str>>,
}

impl Check {
    /// A refusal for a broken rule has the reason `<rule>: <why>`; the rule and the why, when it has that shape.
    pub fn rule(&self) -> Option<(&str, &str)> {
        let (rule, why) = self.reason.split_once(": ")?;
        (!rule.is_empty() && !rule.contains(char::is_whitespace)).then_some((rule, why))
    }
}

/// A call of a tool of an MCP server through the gateway, and what the gateway decided on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mcp {
    pub server: String,
    pub tool: String,
    pub check: Check,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Execution {
    Running,
    Finished {
        failed: bool,
        /// How long the tool ran, as Pi measured it; none when Pi did not say.
        millis: Option<u64>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    pub id: String,
    pub tool: String,
    pub input: Input,
    pub check: Option<Check>,
    /// The gateway's part, when the call is one of an MCP server.
    pub mcp: Option<Box<Mcp>>,
    /// None while only the check has been seen.
    pub execution: Option<Execution>,
    /// What the tool printed: so far while it runs, all of it when it ended.
    pub output: String,
}

impl ToolCall {
    /// Whether the check or the gateway refused the call.
    pub fn refused(&self) -> bool {
        self.verdict().is_some_and(|check| !check.allowed)
    }

    /// The decision the call is shown with: a refusal of either the check or the gateway, else the check's answer,
    /// else the gateway's.
    pub fn verdict(&self) -> Option<&Check> {
        let gateway = self.mcp.as_ref().map(|mcp| &mcp.check);
        [self.check.as_ref(), gateway]
            .into_iter()
            .flatten()
            .find(|check| !check.allowed)
            .or(self.check.as_ref())
            .or(gateway)
    }

    /// The name the call is shown under: `<server> · <tool>` for a call of an MCP server, else the tool's.
    pub fn title(&self) -> String {
        match &self.mcp {
            Some(mcp) => format!("{} · {}", mcp.server, mcp.tool),
            None => self.tool.clone(),
        }
    }
}

/// Who wrote a message of a person.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Said {
    /// A steer sent to a working run.
    Steered,
    /// A message to a chat.
    Said,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    Text(String),
    /// What the agent thought before it acted, as far as the harness reports it.
    Thinking(String),
    Tool(ToolCall),
    Person(Said, String),
    Error(String),
    /// An MCP server of the run's agent could not be reached; its tools are left out.
    McpError {
        server: String,
        message: String,
    },
}

/// The conversation of one run.
#[derive(Debug, Clone, Default)]
pub struct Conversation {
    /// Every event seen, so that a late one can be put in its place and a repeated one dropped.
    events: BTreeMap<i64, RunEvent>,
    entries: Vec<Entry>,
    /// The index of each tool call among `entries`, by the id Pi gave it.
    calls: HashMap<String, usize>,
    /// Whether the last entry is a text that further deltas continue.
    text_open: bool,
    /// Whether the last entry is a thinking that further deltas continue.
    thinking_open: bool,
    /// When each entry began, Unix seconds, in the order of `entries`.
    starts: Vec<i64>,
    /// The time of the event being applied.
    now: i64,
}

impl Conversation {
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// When the entry at `index` began, Unix seconds.
    pub fn started(&self, index: usize) -> i64 {
        self.starts.get(index).copied().unwrap_or_default()
    }

    /// The time of the newest event, Unix seconds; 0 when none.
    pub fn last_at(&self) -> i64 {
        self.events
            .values()
            .map(|event| event.at)
            .max()
            .unwrap_or(0)
    }

    /// Adds an entry that begins now, and returns its index.
    fn add(&mut self, entry: Entry) -> usize {
        self.entries.push(entry);
        self.starts.push(self.now);
        self.entries.len() - 1
    }

    /// Ends the text and the thinking that deltas continue.
    fn close(&mut self) {
        self.text_open = false;
        self.thinking_open = false;
    }

    /// The highest `seq` seen; 0 when none.
    pub fn last_seq(&self) -> i64 {
        self.events.last_key_value().map_or(0, |(seq, _)| *seq)
    }

    /// Adds an event. One seen before is ignored; one older than the newest seen is put in its place.
    pub fn push(&mut self, event: RunEvent) {
        if self.events.contains_key(&event.seq) {
            return;
        }
        if event.seq > self.last_seq() {
            self.apply(&event);
            self.events.insert(event.seq, event);
        } else {
            self.events.insert(event.seq, event);
            let events = std::mem::take(&mut self.events);
            *self = Conversation::default();
            for event in events.into_values() {
                self.apply(&event);
                self.events.insert(event.seq, event);
            }
        }
    }

    /// Sets who decided each check, from the judgements of the run's tool calls, oldest first. A check is matched to
    /// the judgement at the same place in the order, and only when both say the same: a pair that differs is left
    /// without a decider rather than guessed.
    pub fn name_deciders(&mut self, judgements: &[Judgement]) {
        // A call has its check's decision first and the gateway's after it, as the judgements were made.
        let checked = self
            .entries
            .iter_mut()
            .filter_map(|entry| match entry {
                Entry::Tool(call) => Some(call),
                _ => None,
            })
            .flat_map(|call| {
                let gateway = call.mcp.as_mut().map(|mcp| &mut mcp.check);
                call.check.as_mut().into_iter().chain(gateway)
            });
        for (check, judgement) in checked.zip(judgements) {
            if check.allowed == (judgement.choice == "allow") && check.reason == judgement.reason {
                check.decider = Some(judgement.decider.clone());
                check.replay = judgement
                    .outcome
                    .as_deref()
                    .and_then(|outcome| outcome.strip_prefix("replayed: "))
                    .map(Box::from);
            }
        }
    }

    /// How many tool calls the checks or the gateway refused.
    pub fn refusals(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| matches!(entry, Entry::Tool(call) if call.refused()))
            .count()
    }

    /// The tool call with this id, made when it is first seen; calls without an id are always new.
    fn call(&mut self, id: &str, tool: &str) -> &mut ToolCall {
        let known = (!id.is_empty())
            .then(|| self.calls.get(id).copied())
            .flatten();
        let index = known.unwrap_or_else(|| {
            let index = self.add(Entry::Tool(ToolCall {
                id: id.to_string(),
                tool: tool.to_string(),
                input: Input::Unknown,
                check: None,
                mcp: None,
                execution: None,
                output: String::new(),
            }));
            self.calls.insert(id.to_string(), index);
            index
        });
        self.close();
        match &mut self.entries[index] {
            Entry::Tool(call) => call,
            _ => unreachable!("`calls` points at tool calls"),
        }
    }

    fn apply(&mut self, event: &RunEvent) {
        self.now = event.at;
        let data = &event.data;
        let id = data["toolCallId"].as_str().unwrap_or_default();
        let tool = data["toolName"].as_str().unwrap_or_default();
        match event.kind.as_str() {
            "message_update" => {
                let update = &data["assistantMessageEvent"];
                if update["type"] == "text_delta"
                    && let Some(delta) = update["delta"].as_str()
                {
                    match self.entries.last_mut() {
                        Some(Entry::Text(text)) if self.text_open => text.push_str(delta),
                        _ => {
                            self.add(Entry::Text(delta.to_string()));
                        }
                    }
                    self.close();
                    self.text_open = true;
                }
                if update["type"] == "thinking_delta"
                    && let Some(delta) = update["delta"].as_str()
                {
                    match self.entries.last_mut() {
                        Some(Entry::Thinking(text)) if self.thinking_open => text.push_str(delta),
                        _ => {
                            self.add(Entry::Thinking(delta.to_string()));
                        }
                    }
                    self.close();
                    self.thinking_open = true;
                }
            }
            "message_start" | "message_end" | "agent_end" => self.close(),
            "tool_execution_start" => {
                let call = self.call(id, tool);
                call.tool = tool.to_string();
                call.input = Input::of(tool, &data["args"]);
                call.execution = Some(Execution::Running);
            }
            "tool_execution_update" => {
                self.call(id, tool).output = result_text(&data["partialResult"]);
            }
            "tool_execution_end" => {
                let call = self.call(id, tool);
                call.output = result_text(&data["result"]);
                call.execution = Some(Execution::Finished {
                    failed: data["isError"].as_bool().unwrap_or(false),
                    millis: data["durationMs"].as_f64().map(|ms| ms as u64),
                });
            }
            "tool_check" => {
                let id = data["tool_call_id"].as_str().unwrap_or_default();
                let call = self.call(id, data["tool"].as_str().unwrap_or_default());
                call.check = Some(Check {
                    allowed: data["decision"] == "allow",
                    reason: data["reason"].as_str().unwrap_or_default().to_string(),
                    decider: None,
                    replay: None,
                });
            }
            "mcp_call" => self.mcp_call(&event.data),
            "mcp_error" => {
                self.add(Entry::McpError {
                    server: data["server"].as_str().unwrap_or_default().to_string(),
                    message: data["message"].as_str().unwrap_or_default().to_string(),
                });
                self.close();
            }
            "steered" | "said" => {
                let said = if event.kind == "steered" {
                    Said::Steered
                } else {
                    Said::Said
                };
                let message = data["message"].as_str().unwrap_or_default();
                self.add(Entry::Person(said, message.to_string()));
                self.close();
            }
            "moebius_error" => {
                let message = data["message"].as_str().unwrap_or_default();
                self.add(Entry::Error(message.to_string()));
                self.close();
            }
            _ => {}
        }
    }
}

impl Conversation {
    /// A call through the MCP gateway. With Pi-Bolt the harness has the call under its own name, which is
    /// `mcp__moebius__<server>__<tool>`: the gateway's part goes into that call (the one with the same id, else the
    /// latest of that name without a gateway part yet) so that it is drawn once; else it is a call of its own.
    fn mcp_call(&mut self, data: &Value) {
        let text = |key: &str| data[key].as_str().unwrap_or_default().to_string();
        let (server, tool) = (text("server"), text("tool"));
        let id = text("tool_call_id");
        let own_name = format!("__{server}__{tool}");
        let named = |call: &ToolCall| {
            call.mcp.is_none() && call.tool.starts_with("mcp__") && call.tool.ends_with(&own_name)
        };
        let by_id = (!id.is_empty())
            .then(|| self.calls.get(&id).copied())
            .flatten()
            .filter(
                |index| matches!(&self.entries[*index], Entry::Tool(call) if call.mcp.is_none()),
            );
        let by_name = || {
            self.entries
                .iter()
                .rposition(|entry| matches!(entry, Entry::Tool(call) if named(call)))
        };
        let index = by_id.or_else(by_name).unwrap_or_else(|| {
            let index = self.add(Entry::Tool(ToolCall {
                id: id.clone(),
                tool: format!("mcp__{server}__{tool}"),
                input: match &data["input"] {
                    Value::Null => Input::Unknown,
                    input => Input::Other(input.to_string()),
                },
                check: None,
                mcp: None,
                execution: None,
                output: String::new(),
            }));
            if !id.is_empty() {
                self.calls.insert(id, index);
            }
            index
        });
        self.close();
        let Entry::Tool(call) = &mut self.entries[index] else {
            unreachable!("`index` points at a tool call")
        };
        let result = result_text(&data["result"]);
        if call.execution.is_none() {
            call.execution = Some(Execution::Finished {
                failed: data["result"]["isError"].as_bool().unwrap_or(false),
                millis: None,
            });
        }
        if call.output.is_empty() {
            call.output = result;
        }
        call.mcp = Some(Box::new(Mcp {
            server,
            tool,
            check: Check {
                allowed: data["decision"] == "allow",
                reason: text("reason"),
                decider: None,
                replay: None,
            },
        }));
    }
}

/// The text of a tool's result: the text blocks of its content, joined.
fn result_text(result: &Value) -> String {
    result["content"]
        .as_array()
        .map(|blocks| {
            blocks
                .iter()
                .filter_map(|block| block["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn event(seq: i64, kind: &str, data: Value) -> RunEvent {
        RunEvent {
            seq,
            kind: kind.to_string(),
            data,
            at: 1_790_000_000 + seq,
        }
    }

    fn delta(seq: i64, text: &str) -> RunEvent {
        event(
            seq,
            "message_update",
            json!({"assistantMessageEvent": {"type": "text_delta", "contentIndex": 0, "delta": text}}),
        )
    }

    fn start(seq: i64, id: &str, tool: &str, args: Value) -> RunEvent {
        event(
            seq,
            "tool_execution_start",
            json!({"toolCallId": id, "toolName": tool, "args": args}),
        )
    }

    fn end(seq: i64, id: &str, tool: &str, text: &str) -> RunEvent {
        event(
            seq,
            "tool_execution_end",
            json!({"toolCallId": id, "toolName": tool, "isError": false, "durationMs": 400,
                   "result": {"content": [{"type": "text", "text": text}]}}),
        )
    }

    fn check(seq: i64, id: &str, tool: &str, decision: &str, reason: &str) -> RunEvent {
        event(
            seq,
            "tool_check",
            json!({"tool": tool, "tool_call_id": id, "decision": decision, "reason": reason}),
        )
    }

    fn fold(events: Vec<RunEvent>) -> Conversation {
        let mut conversation = Conversation::default();
        for event in events {
            conversation.push(event);
        }
        conversation
    }

    fn call(conversation: &Conversation, index: usize) -> &ToolCall {
        match &conversation.entries()[index] {
            Entry::Tool(call) => call,
            other => panic!("not a tool call: {other:?}"),
        }
    }

    /// A run like the mockup's: text, a read, a refused edit, an edit, a bash that still runs, a steer.
    fn run() -> Vec<RunEvent> {
        vec![
            delta(1, "Review found "),
            delta(2, "a problem."),
            check(3, "c1", "read", "allow", "no rule is broken"),
            start(4, "c1", "read", json!({"path": "core/tree.cpp"})),
            end(5, "c1", "read", "int main() {}"),
            check(
                6,
                "c2",
                "edit",
                "refuse",
                "no-vendor-edits: never edit vendor/",
            ),
            start(
                7,
                "c2",
                "edit",
                json!({"path": "vendor/a.hpp", "edits": [{"oldText": "a", "newText": "b"}]}),
            ),
            end(8, "c2", "edit", "blocked"),
            delta(9, "Right."),
            check(10, "c3", "bash", "allow", "no rule is broken"),
            start(
                11,
                "c3",
                "bash",
                json!({"command": "cmake --build build\nctest"}),
            ),
            event(
                12,
                "tool_execution_update",
                json!({"toolCallId": "c3", "toolName": "bash",
                       "partialResult": {"content": [{"type": "text", "text": "[ 92%] Linking"}]}}),
            ),
            event(13, "steered", json!({"message": "also test depth 0"})),
            event(14, "moebius_error", json!({"message": "it broke"})),
        ]
    }

    #[test]
    fn deltas_of_a_message_are_joined() {
        let conversation = fold(vec![delta(1, "line one "), delta(2, "line two")]);
        assert_eq!(
            conversation.entries(),
            [Entry::Text("line one line two".into())]
        );
    }

    #[test]
    fn a_message_end_or_a_tool_call_starts_a_new_text() {
        let conversation = fold(vec![
            delta(1, "one"),
            event(2, "message_end", json!({})),
            delta(3, "two"),
            start(4, "c", "read", json!({"path": "a"})),
            delta(5, "three"),
        ]);
        let texts: Vec<_> = conversation
            .entries()
            .iter()
            .filter_map(|e| match e {
                Entry::Text(text) => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(texts, ["one", "two", "three"]);
    }

    #[test]
    fn a_tool_call_pairs_with_its_check_and_its_end() {
        let conversation = fold(run());
        let read = call(&conversation, 1);
        assert_eq!(read.input, Input::Read("core/tree.cpp".into()));
        assert_eq!(
            read.check,
            Some(Check {
                allowed: true,
                reason: "no rule is broken".into(),
                decider: None,
                replay: None,
            })
        );
        assert_eq!(
            read.execution,
            Some(Execution::Finished {
                failed: false,
                millis: Some(400)
            })
        );
        assert_eq!(read.output, "int main() {}");
        // The check came first, the start second: still one call.
        assert_eq!(conversation.entries().len(), 7);
    }

    #[test]
    fn a_refused_call_names_its_rule_and_a_running_one_shows_its_output_so_far() {
        let conversation = fold(run());
        let refused = call(&conversation, 2);
        assert!(refused.refused());
        assert_eq!(
            refused.check.as_ref().unwrap().rule(),
            Some(("no-vendor-edits", "never edit vendor/"))
        );
        assert_eq!(conversation.refusals(), 1);
        let bash = call(&conversation, 4);
        assert_eq!(bash.execution, Some(Execution::Running));
        assert_eq!(bash.output, "[ 92%] Linking");
        assert_eq!(bash.input.summary(), "cmake --build build");
    }

    #[test]
    fn a_reason_that_is_not_a_rule_has_no_rule_name() {
        let check = Check {
            allowed: false,
            reason: "the rules cannot be read, so no call is allowed: x".into(),
            decider: None,
            replay: None,
        };
        assert_eq!(check.rule(), None);
    }

    #[test]
    fn edits_of_both_shapes_become_changes() {
        let many = Input::of(
            "edit",
            &json!({"path": "a", "edits": [{"oldText": "1", "newText": "2"}, {"oldText": "3", "newText": "4"}]}),
        );
        let single = Input::of(
            "edit",
            &json!({"path": "a", "oldText": "1", "newText": "2"}),
        );
        assert!(matches!(many, Input::Edit { changes, .. } if changes.len() == 2));
        assert_eq!(
            single,
            Input::Edit {
                path: "a".into(),
                changes: vec![("1".into(), "2".into())]
            }
        );
    }

    #[test]
    fn steers_and_messages_and_errors_are_entries() {
        let conversation = fold(vec![
            event(1, "said", json!({"message": "Hello"})),
            event(2, "steered", json!({"message": "Also this"})),
            event(3, "moebius_error", json!({"message": "it broke"})),
        ]);
        assert_eq!(
            conversation.entries(),
            [
                Entry::Person(Said::Said, "Hello".into()),
                Entry::Person(Said::Steered, "Also this".into()),
                Entry::Error("it broke".into()),
            ]
        );
    }

    #[test]
    fn events_out_of_order_or_twice_give_the_same_conversation() {
        let ordered = fold(run());
        let mut shuffled = run();
        shuffled.reverse();
        shuffled.extend(run());
        let shuffled = fold(shuffled);
        assert_eq!(ordered.entries(), shuffled.entries());
        assert_eq!(shuffled.last_seq(), 14);
    }

    #[test]
    fn a_page_and_the_stream_give_the_same_conversation() {
        let mut streamed = Conversation::default();
        for event in run() {
            streamed.push(event);
        }
        assert_eq!(streamed.entries(), fold(run()).entries());
    }

    #[test]
    fn thinking_deltas_are_joined_and_steps_know_when_they_began() {
        let thought = |seq, text: &str| {
            event(
                seq,
                "message_update",
                json!({"assistantMessageEvent": {"type": "thinking_delta", "delta": text}}),
            )
        };
        let conversation = fold(vec![
            thought(1, "Look at "),
            thought(2, "the tree."),
            start(3, "c", "read", json!({"path": "a"})),
            delta(4, "Done."),
        ]);
        assert_eq!(
            conversation.entries()[0],
            Entry::Thinking("Look at the tree.".into())
        );
        assert_eq!(conversation.entries().len(), 3);
        assert_eq!(conversation.started(0), 1_790_000_001);
        assert_eq!(conversation.started(1), 1_790_000_003);
        assert_eq!(conversation.last_at(), 1_790_000_004);
    }

    #[test]
    fn a_decider_is_named_only_for_a_pair_that_agrees() {
        let mut conversation = fold(run());
        let judgement = |choice: &str, reason: &str, decider: &str| Judgement {
            kind: "tool_call".into(),
            choice: choice.into(),
            reason: reason.into(),
            decider: decider.into(),
            outcome: None,
            run: Some(1),
        };
        conversation.name_deciders(&[
            judgement("allow", "no rule is broken", "patterns"),
            judgement("refuse", "something else", "teacher"),
            judgement("allow", "no rule is broken", "teacher"),
        ]);
        let decider = |index| {
            call(&conversation, index)
                .check
                .as_ref()
                .unwrap()
                .decider
                .clone()
        };
        assert_eq!(decider(1), Some("patterns".to_string()));
        assert_eq!(decider(2), None);
        assert_eq!(decider(4), Some("teacher".to_string()));
    }

    #[test]
    fn a_refusal_that_was_replayed_carries_what_came_of_it() {
        let mut conversation = fold(run());
        let judgement = |choice: &str, reason: &str, outcome: Option<&str>| Judgement {
            kind: "tool_call".into(),
            choice: choice.into(),
            reason: reason.into(),
            decider: "teacher".into(),
            outcome: outcome.map(str::to_string),
            run: Some(1),
        };
        let reason = call(&conversation, 2)
            .check
            .as_ref()
            .unwrap()
            .reason
            .clone();
        conversation.name_deciders(&[
            judgement("allow", "no rule is broken", Some("merged")),
            judgement("refuse", &reason, Some("replayed: exit 0; deleted 3 files")),
        ]);
        let replay = |index| {
            call(&conversation, index)
                .check
                .as_ref()
                .unwrap()
                .replay
                .clone()
        };
        assert_eq!(replay(1), None, "only a replay is shown");
        assert_eq!(replay(2).as_deref(), Some("exit 0; deleted 3 files"));
    }

    fn mcp(seq: i64, id: &str, server: &str, tool: &str, decision: &str, reason: &str) -> RunEvent {
        let text = if decision == "allow" {
            "3 open issues".to_string()
        } else {
            format!("Moebius refused this call: {reason}")
        };
        event(
            seq,
            "mcp_call",
            json!({"server": server, "tool": tool, "tool_call_id": id, "input": {"query": "bug"},
                   "decision": decision, "reason": reason,
                   "result": {"content": [{"type": "text", "text": text}], "isError": decision != "allow"}}),
        )
    }

    #[test]
    fn an_mcp_call_is_folded_into_pis_call_of_the_same_tool() {
        let conversation = fold(vec![
            check(
                1,
                "p1",
                "mcp__moebius__github__search",
                "allow",
                "no rule is broken",
            ),
            start(
                2,
                "p1",
                "mcp__moebius__github__search",
                json!({"query": "bug"}),
            ),
            mcp(3, "mcp-7", "github", "search", "allow", "no rule is broken"),
            end(4, "p1", "mcp__moebius__github__search", "3 open issues"),
        ]);
        assert_eq!(conversation.entries().len(), 1);
        let call = call(&conversation, 0);
        assert_eq!(call.title(), "github · search");
        assert_eq!(call.output, "3 open issues");
        assert!(!call.refused());
        assert_eq!(
            call.execution,
            Some(Execution::Finished {
                failed: false,
                millis: Some(400)
            })
        );
    }

    #[test]
    fn an_mcp_call_with_the_id_of_pis_call_is_folded_into_it() {
        let conversation = fold(vec![
            start(
                1,
                "same",
                "mcp__moebius__github__search",
                json!({"query": "bug"}),
            ),
            mcp(
                2,
                "same",
                "github",
                "search",
                "refuse",
                "no-mcp-writes: read only",
            ),
        ]);
        assert_eq!(conversation.entries().len(), 1);
        assert!(call(&conversation, 0).refused());
        assert_eq!(conversation.refusals(), 1);
    }

    #[test]
    fn an_mcp_call_without_a_call_of_pis_is_a_call_of_its_own() {
        let conversation = fold(vec![
            mcp(1, "mcp-1", "github", "search", "allow", "no rule is broken"),
            mcp(
                2,
                "mcp-2",
                "github",
                "search",
                "refuse",
                "no-mcp-writes: read only",
            ),
        ]);
        assert_eq!(conversation.entries().len(), 2);
        let first = call(&conversation, 0);
        assert_eq!(first.title(), "github · search");
        assert_eq!(first.input, Input::Other("{\"query\":\"bug\"}".into()));
        assert_eq!(first.output, "3 open issues");
        assert!(matches!(
            first.execution,
            Some(Execution::Finished { failed: false, .. })
        ));
        let refused = call(&conversation, 1);
        assert!(refused.refused());
        assert_eq!(
            refused.verdict().unwrap().rule(),
            Some(("no-mcp-writes", "read only"))
        );
        assert_eq!(conversation.refusals(), 1);
    }

    #[test]
    fn two_mcp_calls_go_into_two_calls_of_pi_in_order() {
        let name = "mcp__moebius__github__search";
        let conversation = fold(vec![
            start(1, "a", name, json!({})),
            mcp(2, "mcp-1", "github", "search", "allow", "no rule is broken"),
            start(3, "b", name, json!({})),
            mcp(
                4,
                "mcp-2",
                "github",
                "search",
                "refuse",
                "no-mcp-writes: read only",
            ),
        ]);
        assert_eq!(conversation.entries().len(), 2);
        assert!(!call(&conversation, 0).refused());
        assert!(call(&conversation, 1).refused());
    }

    #[test]
    fn an_mcp_error_is_an_entry_of_its_own() {
        let conversation = fold(vec![event(
            1,
            "mcp_error",
            json!({"server": "github", "message": "could not start"}),
        )]);
        assert_eq!(
            conversation.entries(),
            [Entry::McpError {
                server: "github".into(),
                message: "could not start".into()
            }]
        );
    }

    #[test]
    fn a_decider_is_named_for_the_gateway_decision_too() {
        let mut conversation = fold(vec![
            mcp(1, "mcp-1", "github", "search", "allow", "no rule is broken"),
            check(2, "c", "read", "allow", "no rule is broken"),
        ]);
        let judgement = |decider: &str| Judgement {
            kind: "tool_call".into(),
            choice: "allow".into(),
            reason: "no rule is broken".into(),
            decider: decider.into(),
            outcome: None,
            run: Some(1),
        };
        conversation.name_deciders(&[judgement("patterns"), judgement("teacher")]);
        assert_eq!(
            call(&conversation, 0).verdict().unwrap().decider.as_deref(),
            Some("patterns")
        );
        assert_eq!(
            call(&conversation, 1).verdict().unwrap().decider.as_deref(),
            Some("teacher")
        );
    }
}
