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
}

impl Check {
    /// A refusal for a broken rule has the reason `<rule>: <why>`; the rule and the why, when it has that shape.
    pub fn rule(&self) -> Option<(&str, &str)> {
        let (rule, why) = self.reason.split_once(": ")?;
        (!rule.is_empty() && !rule.contains(char::is_whitespace)).then_some((rule, why))
    }
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
    /// None while only the check has been seen.
    pub execution: Option<Execution>,
    /// What the tool printed: so far while it runs, all of it when it ended.
    pub output: String,
}

impl ToolCall {
    pub fn refused(&self) -> bool {
        self.check.as_ref().is_some_and(|check| !check.allowed)
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
    Tool(ToolCall),
    Person(Said, String),
    Error(String),
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
}

impl Conversation {
    pub fn entries(&self) -> &[Entry] {
        &self.entries
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
        let checked = self.entries.iter_mut().filter_map(|entry| match entry {
            Entry::Tool(call) => call.check.as_mut(),
            _ => None,
        });
        for (check, judgement) in checked.zip(judgements) {
            if check.allowed == (judgement.choice == "allow") && check.reason == judgement.reason {
                check.decider = Some(judgement.decider.clone());
            }
        }
    }

    /// How many tool calls the checks refused.
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
            self.entries.push(Entry::Tool(ToolCall {
                id: id.to_string(),
                tool: tool.to_string(),
                input: Input::Unknown,
                check: None,
                execution: None,
                output: String::new(),
            }));
            self.calls.insert(id.to_string(), self.entries.len() - 1);
            self.entries.len() - 1
        });
        self.text_open = false;
        match &mut self.entries[index] {
            Entry::Tool(call) => call,
            _ => unreachable!("`calls` points at tool calls"),
        }
    }

    fn apply(&mut self, event: &RunEvent) {
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
                        _ => self.entries.push(Entry::Text(delta.to_string())),
                    }
                    self.text_open = true;
                }
            }
            "message_start" | "message_end" | "agent_end" => self.text_open = false,
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
                });
            }
            "steered" | "said" => {
                let said = if event.kind == "steered" {
                    Said::Steered
                } else {
                    Said::Said
                };
                let message = data["message"].as_str().unwrap_or_default();
                self.entries.push(Entry::Person(said, message.to_string()));
                self.text_open = false;
            }
            "moebius_error" => {
                let message = data["message"].as_str().unwrap_or_default();
                self.entries.push(Entry::Error(message.to_string()));
                self.text_open = false;
            }
            _ => {}
        }
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
                decider: None
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
    fn a_decider_is_named_only_for_a_pair_that_agrees() {
        let mut conversation = fold(run());
        let judgement = |choice: &str, reason: &str, decider: &str| Judgement {
            kind: "tool_call".into(),
            choice: choice.into(),
            reason: reason.into(),
            decider: decider.into(),
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
}
