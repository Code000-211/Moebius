//! A skill's other files: the files of `skills/<name>/` beside its `SKILL.md`, read as the project page reads a
//! repository's files (see [`crate::repos::files`]), and written or deleted by a person or the Moebius agent as commits
//! on the default branch, pushed to `origin`, the same way as every other edit of the clone (see `edits`). `SKILL.md`
//! itself is written with the skill (`PUT /v1/skills/{name}`), and a built-in skill's files are read from the binary.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::edits::Edit;
use super::{HarnessRepo, SkillView};
use crate::app::{App, AppError, Result};
use crate::config::valid_agent_name;
use crate::decide::Actor;
use crate::harness::skill;
use crate::repos::files::{FileView, TEXT_LIMIT, inside};

/// What `PUT /v1/skills/{name}/files/{path}` takes.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillFileText {
    pub text: String,
}

impl HarnessRepo {
    /// The file at `path` of the skill `name`: its text, or for a binary or large file only its size.
    pub fn skill_file(&self, name: &str, path: &str) -> Result<FileView> {
        let _read_at = self.lock();
        let path = file_path(path)?;
        let not_found = || AppError::NotFound(format!("file {path} of the skill {name}"));
        if let Some(built_in) = skill::built_in(name).or_else(|| self.vendored(name)) {
            let text = built_in.file(&path).ok_or_else(not_found)?;
            return Ok(FileView {
                path: path.clone(),
                size: text.len() as u64,
                text: Some(text.into()),
                binary: false,
                too_large: false,
            });
        }
        let file = self.skill_file_path(name, &path)?;
        if !file.is_file() {
            return Err(not_found());
        }
        let size = std::fs::metadata(&file)?.len();
        let mut view = FileView {
            path,
            size,
            text: None,
            binary: false,
            too_large: false,
        };
        if size > TEXT_LIMIT {
            view.too_large = true;
            return Ok(view);
        }
        match String::from_utf8(std::fs::read(&file)?) {
            Ok(text) if !text.contains('\0') => view.text = Some(text),
            _ => view.binary = true,
        }
        Ok(view)
    }

    /// A person or the Moebius agent writes the file at `path` of the skill `name`, creating it and its directories
    /// when they are new: committed on the default branch and pushed to `origin`. Text that changes nothing makes no
    /// commit. The skill is given back with its files.
    pub fn save_skill_file(
        &self,
        app: &App,
        by: &Actor,
        name: &str,
        path: &str,
        new: &SkillFileText,
    ) -> Result<SkillView> {
        let mut read_at = self.lock();
        let path = file_path(path)?;
        let file = self.editable_skill_file(name, &path)?;
        if file.is_dir() {
            return Err(AppError::Invalid(format!("{path} is a directory")));
        }
        let verb = if file.is_file() { "Update" } else { "Add" };
        let message = format!("{verb} {path} of the skill {name}");
        let context = BTreeMap::from([
            ("skill".to_string(), name.to_string()),
            ("file".to_string(), path.clone()),
            ("text".to_string(), new.text.clone()),
        ]);
        let write = || {
            std::fs::create_dir_all(file.parent().expect("a skill's file is in its directory"))?;
            Ok(std::fs::write(&file, &new.text)?)
        };
        let files = format!("skills/{name}");
        let edit = Edit {
            kind: "edit_skill",
            files: &files,
            message: &message,
        };
        let commit = self.commit(app, by, &mut read_at, &edit, context, write)?;
        self.skill_after_edit(name, commit)
    }

    /// A person or the Moebius agent deletes the file at `path` of the skill `name`, and the directories it leaves
    /// empty, the same way.
    pub fn delete_skill_file(
        &self,
        app: &App,
        by: &Actor,
        name: &str,
        path: &str,
    ) -> Result<SkillView> {
        let mut read_at = self.lock();
        let path = file_path(path)?;
        let file = self.editable_skill_file(name, &path)?;
        if !file.is_file() {
            return Err(AppError::NotFound(format!(
                "file {path} of the skill {name}"
            )));
        }
        let dir = self.path.join("skills").join(name);
        let write = || {
            std::fs::remove_file(&file)?;
            let mut parent = file.parent();
            while let Some(empty) = parent.filter(|p| *p != dir) {
                if std::fs::remove_dir(empty).is_err() {
                    break;
                }
                parent = empty.parent();
            }
            Ok(())
        };
        let message = format!("Delete {path} of the skill {name}");
        let context = BTreeMap::from([
            ("skill".to_string(), name.to_string()),
            ("file".to_string(), path.clone()),
        ]);
        let files = format!("skills/{name}");
        let edit = Edit {
            kind: "edit_skill",
            files: &files,
            message: &message,
        };
        let commit = self.commit(app, by, &mut read_at, &edit, context, write)?;
        self.skill_after_edit(name, commit)
    }

    /// Where the file at `path` of the skill `name` of the repository is, refused when the skill is built in.
    fn editable_skill_file(&self, name: &str, path: &str) -> Result<PathBuf> {
        if skill::built_in(name)
            .or_else(|| self.vendored(name))
            .is_some()
        {
            return Err(AppError::Conflict(format!(
                "the skill {name} is built in and read only"
            )));
        }
        self.skill_file_path(name, path)
    }

    /// Where the file at `path` of the skill `name` is in the clone: the skill must be there, and nothing on the way
    /// may be a link, which could lead out of the repository.
    fn skill_file_path(&self, name: &str, path: &str) -> Result<PathBuf> {
        let skills = self.path.join("skills");
        let dir = skills.join(name);
        if !valid_agent_name(name)
            || is_link(&skills)
            || is_link(&dir)
            || !dir.join("SKILL.md").is_file()
        {
            return Err(AppError::NotFound(format!("skill {name}")));
        }
        let mut walked = dir;
        for part in path.split('/') {
            walked.push(part);
            if is_link(&walked) {
                return Err(AppError::Invalid(format!(
                    "{path} goes through a link, which is not edited here"
                )));
            }
        }
        Ok(walked)
    }

    /// The skill `name` as it is after an edit, with the commit the edit made.
    fn skill_after_edit(&self, name: &str, commit: Option<String>) -> Result<SkillView> {
        let page = std::fs::read_to_string(self.path.join("skills").join(name).join("SKILL.md"))?;
        Ok(self.skill_view(name, &page, commit))
    }
}

/// `path` as the path of one of a skill's other files: inside its directory, and not its `SKILL.md`.
fn file_path(path: &str) -> Result<String> {
    let path = inside(path).map_err(|_| {
        AppError::Invalid(format!("{path} is not a path inside the skill's directory"))
    })?;
    match path.as_str() {
        "" => Err(AppError::Invalid("a file needs a path".into())),
        "SKILL.md" => Err(AppError::Invalid(
            "SKILL.md is written with the skill (PUT /v1/skills/{name})".into(),
        )),
        _ => Ok(path),
    }
}

fn is_link(path: &Path) -> bool {
    path.symlink_metadata()
        .is_ok_and(|m| m.file_type().is_symlink())
}
