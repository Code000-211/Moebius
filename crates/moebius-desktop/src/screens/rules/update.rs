//! How a message changes the Rules page: the rules read, the request to the Moebius agent, and a rule edited in its
//! card, saved or deleted as a save of its whole file.

use iced::Task;
use iced::widget::text_editor;

use crate::app::Message as AppMessage;
use crate::client::Client;
use crate::screens::live::say;
use crate::settings::Settings;

use super::file::valid_name;
use super::{Editing, Message, Notice, Place, Rules};

/// What the Moebius agent is asked, with what the person said.
fn request(wanted: &str) -> String {
    format!(
        "Change the server's rules in prose. Read the current rules with `\"$MOEBIUS\" rules` and the rules guide of \
         the moebius skill first. Write the change as rules in prose, each a `## name` heading and a few plain \
         sentences, keep every existing rule as it is unless the person asks to change or remove it, and never \
         weaken a rule on your own. Write the whole rules file to a file and save it with \
         `\"$MOEBIUS\" rules save <file>`. Then say what changed.\n\nWhat should be allowed or forbidden:\n\n{wanted}"
    )
}

impl Rules {
    pub fn update(&mut self, message: Message, settings: Option<&Settings>) -> Task<AppMessage> {
        match message {
            Message::Loaded(Ok(view)) => self.view = Some(view),
            Message::Loaded(Err(_)) => {}
            Message::ByAgent(by_agent) => self.by_agent = by_agent,
            Message::WishEdited(action) => self.wish.perform(action),
            Message::AskAgent => {
                let wanted = self.wish.text().trim().to_string();
                let (Some(settings), false, false) = (settings, self.asking, wanted.is_empty())
                else {
                    return Task::none();
                };
                self.notice = None;
                self.asking = true;
                let (client, message) = (Client::new(settings.clone()), request(&wanted));
                return Task::perform(
                    async move { client.ask(&message, None, "").await },
                    |result| AppMessage::Rules(Message::Asked(result)),
                );
            }
            Message::Asked(Ok(_)) => {
                self.asking = false;
                self.by_agent = false;
                self.wish = text_editor::Content::new();
            }
            Message::Asked(Err(e)) => {
                self.asking = false;
                self.notice = Some((Place::Server, Notice::Failed(say(&e))));
            }
            Message::EditRule(place, rule) => {
                let (name, text) = match &rule {
                    Some(name) => match self.rules_of(place).iter().find(|r| &r.name == name) {
                        Some(found) => (found.name.clone(), found.text.clone()),
                        None => return Task::none(),
                    },
                    None => (String::new(), String::new()),
                };
                self.notice = None;
                self.editing = Some(Editing {
                    place,
                    rule,
                    name,
                    text: text_editor::Content::with_text(&text),
                    deleting: false,
                    saving: false,
                    error: None,
                });
            }
            Message::NameEdited(name) => {
                if let Some(editing) = &mut self.editing {
                    editing.name = name;
                }
            }
            Message::TextEdited(action) => {
                if let Some(editing) = &mut self.editing {
                    editing.text.perform(action);
                }
            }
            Message::CancelRule => self.editing = None,
            Message::AskDelete => {
                if let Some(editing) = &mut self.editing {
                    editing.deleting = true;
                }
            }
            Message::KeepRule => {
                if let Some(editing) = &mut self.editing {
                    editing.deleting = false;
                }
            }
            Message::SaveRule => return self.save(false, settings),
            Message::DeleteRule => return self.save(true, settings),
            Message::RuleSaved(Ok(commit)) => {
                if let Some(editing) = self.editing.take() {
                    self.notice = Some((editing.place, Notice::Saved(commit)));
                }
                if let Some(settings) = settings {
                    return self.reload(settings);
                }
            }
            Message::RuleSaved(Err(e)) => {
                if let Some(editing) = &mut self.editing {
                    editing.saving = false;
                    editing.deleting = false;
                    editing.error = Some(say(&e));
                }
            }
        }
        Task::none()
    }

    /// Saves the file of the rule being edited, with the rule changed, added or, with `delete`, dropped.
    fn save(&mut self, delete: bool, settings: Option<&Settings>) -> Task<AppMessage> {
        let (Some(settings), Some(file)) = (settings, self.file_to_save(delete)) else {
            return Task::none();
        };
        let Some(editing) = self.editing.as_mut().filter(|e| !e.saving) else {
            return Task::none();
        };
        if !delete && !valid_name(&editing.name) {
            return Task::none();
        }
        editing.saving = true;
        editing.error = None;
        let (client, place) = (Client::new(settings.clone()), editing.place);
        Task::perform(
            async move {
                match place {
                    Place::Server => client.save_rules(&file).await.map(|view| view.commit),
                    Place::Repository(id) => client
                        .save_file(id, ".moebius/rules.md", &file)
                        .await
                        .map(|_| None),
                }
            },
            |result| AppMessage::Rules(Message::RuleSaved(result)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_agent_is_told_to_keep_the_existing_rules_and_save_the_file() {
        let asked = request("never touch the lockfile");
        assert!(asked.contains(r#"`"$MOEBIUS" rules`"#));
        assert!(asked.contains(r#"`"$MOEBIUS" rules save <file>`"#));
        assert!(asked.contains("never weaken"));
        assert!(
            asked.ends_with("What should be allowed or forbidden:\n\nnever touch the lockfile")
        );
    }
}
