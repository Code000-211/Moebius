/**
 * The editor of one skill: the head (Save and Delete; "Built in · read only" for a skill Moebius brings, with
 * "Update from the repository" for one copied from a repository), its description, the list of its files with
 * "New file", and
 * the text of `SKILL.md` or the editor of the file open (`FileEditor`). What a person types stays with the page while
 * it is unsaved, so moving to another skill — or another file — does not lose it.
 */
import { FileIcon } from "@phosphor-icons/react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Link } from "react-router";
import { startChat } from "../../api/calls/chat";
import { deleteSkill, saveSkill } from "../../api/calls/harness";
import { keys } from "../../api/keys";
import type { SkillDetail, SkillSource } from "../../api/types/harness";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { Box } from "../../ui/page/Box";
import { Field } from "../../ui/page/Field";
import { MarkdownBody, type MarkdownView } from "../../ui/page/MarkdownBody";
import { Notice } from "../../ui/page/Notice";
import { Status } from "../../ui/page/Status";
import { Committed, EditorHead, useDraft } from "../harness/form";

import { updateRequest } from "../harness/importRequest";
import { FileEditor } from "./FileEditor";
import { skillRequest } from "./model";

/** What the editor of a skill holds that the repository does not have yet. */
export interface SkillDraft {
  name: string;
  naming: boolean;
  exists: boolean;
  description: string;
  text: string;
  files: string[];
  /** The file open beside SKILL.md; null while SKILL.md is. */
  openFile: string | null;
  /** A new file is being named. */
  newFile: boolean;
}

export function SkillEditor({
  name: initialName,
  naming: initialNaming,
  builtIn,
  source,
  detail,
  saved,
  draft,
  drafts,
  onNamed,
  onDeleted,
}: {
  name: string;
  naming: boolean;
  builtIn: boolean;
  source: SkillSource | undefined;
  detail?: SkillDetail;
  /** The commit of the save that made this skill open (null for none); undefined when it was not just saved. */
  saved?: string | null;
  /** What was typed for this skill before, or null when nothing was. */
  draft: SkillDraft | null;
  /** Where the page keeps what is typed here. */
  drafts?: { hold: (draft: SkillDraft) => void; drop: () => void };
  onNamed: (name: string, commit: string | null) => void;
  onDeleted: () => void;
}) {
  const queryClient = useQueryClient();
  // What the repository has for this skill, so Save can say whether anything is waiting for it.
  const fromFile: SkillDraft = {
    name: initialName,
    naming: initialNaming,
    exists: detail !== undefined && !builtIn,
    description: detail?.description ?? "",
    text: detail?.text ?? "",
    files: detail?.files ?? [],
    openFile: null,
    newFile: false,
  };
  const [state, setState] = useState<SkillDraft>(draft ?? fromFile);
  const [baseline, setBaseline] = useState<SkillDraft>(fromFile);
  const [notice, setNotice] = useState<{ error: string } | { commit: string | null } | null>(
    saved !== undefined ? { commit: saved } : null,
  );
  // What the last save of a file made: its commit (null for none); undefined while nothing was saved.
  const [fileSaved, setFileSaved] = useState<string | null | undefined>(undefined);
  // How a Markdown file of this skill is read. It belongs to the skill rather than to the file open, so moving along
  // the list does not send a person back to the tab they had already chosen.
  const [view, setView] = useState<MarkdownView>("raw");

  const { name, naming, exists, description, text, files, openFile, newFile } = state;
  // Only what a save sends: which file is open is a look, not an edit.
  const savedOf = (held: SkillDraft) =>
    JSON.stringify([held.name, held.exists, held.description, held.text, held.files]);
  const dirty = savedOf(state) !== savedOf(baseline);
  useDraft(state, dirty, drafts);

  const patch = (changes: Partial<SkillDraft>) => setState((held) => ({ ...held, ...changes }));
  /** A field changed: the draft follows, and the last outcome is no longer about this. */
  const field =
    <K extends keyof SkillDraft>(key: K) =>
    (value: SkillDraft[K]) => {
      patch({ [key]: value } as Partial<SkillDraft>);
      setNotice(null);
    };

  const save = useMutation({
    mutationFn: () => {
      const request = skillRequest({ name, description, text });
      if (typeof request === "string") throw new Error(request);
      return saveSkill(request.name, request.description, request.text);
    },
    onSuccess: async (saved) => {
      const next: SkillDraft = {
        ...state,
        name: saved.name,
        naming: false,
        exists: true,
        files: saved.files,
      };
      setState(next);
      setBaseline(next);
      setNotice({ commit: saved.commit ?? null });
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: keys.skills }),
        queryClient.invalidateQueries({ queryKey: keys.skill(saved.name) }),
      ]);
      onNamed(saved.name, saved.commit ?? null);
    },
    onError: (error) => setNotice({ error: error.message }),
  });
  const remove = useMutation({
    mutationFn: () => deleteSkill(name),
    onSuccess: async () => {
      drafts?.drop();
      await queryClient.invalidateQueries({ queryKey: keys.skills });
      onDeleted();
    },
    onError: (error) => setNotice({ error: error.message }),
  });
  const update = useMutation({
    mutationFn: () => {
      if (!source) throw new Error("This skill has no source.");
      return startChat({ agent: "Moebius", message: updateRequest(name, source) });
    },
    onError: (error) => setNotice({ error: error.message }),
  });

  const head = (
    <Box className="flex flex-col gap-4">
      <EditorHead
        name={name}
        naming={naming}
        exists={exists}
        deleteLabel="Delete skill"
        namePlaceholder="Name of the new skill"
        onName={field("name")}
        saving={save.isPending}
        canSave
        dirty={dirty}
        onSave={() => save.mutate()}
        onDelete={() => remove.mutate()}
        deleting={remove.isPending}
        readOnly={
          builtIn ? (
            <div className="flex flex-wrap items-center gap-3">
              <Status>Built in · read only</Status>
              {source && (
                <Button
                  variant="secondary"
                  size="sm"
                  disabled={update.isPending || update.isSuccess}
                  onClick={() => update.mutate()}
                >
                  {update.isPending ? "Updating" : "Update from the repository"}
                </Button>
              )}
            </div>
          ) : undefined
        }
      />
      {builtIn ? (
        <p className="m-0 max-w-prose text-base text-secondary">{description}</p>
      ) : (
        <Field label="Description" hint="One line an agent reads to decide when the skill applies.">
          {(id) => (
            <Input
              id={id}
              value={description}
              placeholder="One line that says when to use the skill"
              onChange={(event) => field("description")(event.target.value)}
            />
          )}
        </Field>
      )}
      {source && (
        <p className="m-0 max-w-prose break-words text-sm text-secondary">
          Copied from <span className="font-mono text-sm">{source.url}</span> at commit{" "}
          <span className="font-mono text-sm">{source.commit.slice(0, 12)}</span>. A skill of this
          name in the harness repository replaces it.
        </p>
      )}
      {update.isSuccess && (
        <div className="flex flex-wrap items-center gap-3">
          <Notice tone="ok">The Moebius agent has the request.</Notice>
          <Button asChild variant="secondary" size="sm">
            <Link to={`/chat/${update.data.id}`}>Open the chat</Link>
          </Button>
        </div>
      )}
      {notice &&
        ("error" in notice ? (
          <Notice tone="error">{notice.error}</Notice>
        ) : (
          <Committed commit={notice.commit} />
        ))}
    </Box>
  );

  const editing = openFile !== null || newFile;
  return (
    <div className="flex flex-col gap-8">
      {head}
      <Box
        title="Files"
        aside={builtIn ? "Read only" : !exists ? "Save the skill to add files" : undefined}
        actions={
          !builtIn && exists ? (
            <Button
              variant="secondary"
              size="sm"
              onClick={() => {
                patch({ openFile: null, newFile: true });
                setFileSaved(undefined);
              }}
            >
              New file
            </Button>
          ) : undefined
        }
        className="flex flex-col gap-2"
        padded
      >
        {/* biome-ignore lint/a11y/noRedundantRoles: preflight sets list-style: none, and WebKit then drops a list with no marker from the accessibility tree entirely */}
        <ul role="list" className="m-0 flex list-none flex-col gap-1 p-0">
          {["SKILL.md", ...files].map((path) => {
            const open = path === "SKILL.md" ? !editing : openFile === path;
            return (
              <li key={path}>
                <button
                  type="button"
                  aria-current={open ? "true" : undefined}
                  onClick={() => {
                    patch({
                      newFile: false,
                      openFile: path === "SKILL.md" ? null : path,
                    });
                    setFileSaved(undefined);
                  }}
                  className={`flex min-h-10 w-full cursor-pointer items-center gap-3 rounded-control px-3 text-left font-mono text-sm transition-colors duration-(--motion-routine) ease-out ${
                    open ? "bg-selected text-text" : "text-secondary hover:bg-hover hover:text-text"
                  }`}
                >
                  <FileIcon className="size-4 shrink-0 text-secondary" />
                  <span className="min-w-0 break-all">{path}</span>
                </button>
              </li>
            );
          })}
        </ul>
      </Box>
      {editing ? (
        <FileEditor
          key={newFile ? "new" : openFile}
          skill={name}
          path={newFile ? "" : (openFile as string)}
          naming={newFile}
          builtIn={builtIn}
          saved={fileSaved}
          view={view}
          onViewChange={setView}
          onSaved={(saved, path) => {
            patch({ files: saved.files, newFile: false, openFile: path });
            setBaseline((held) => ({ ...held, files: saved.files }));
            setFileSaved(saved.commit ?? null);
          }}
          onDeleted={(saved) => {
            patch({ files: saved.files, newFile: false, openFile: null });
            setBaseline((held) => ({ ...held, files: saved.files }));
            setNotice({ commit: saved.commit ?? null });
          }}
        />
      ) : (
        <Box title="SKILL.md" className="min-w-0">
          <MarkdownBody
            label="SKILL.md"
            text={text}
            readOnly={builtIn}
            view={view}
            onViewChange={setView}
            placeholder="What the skill teaches the agent, in Markdown."
            onChange={(next) => field("text")(next)}
          />
        </Box>
      )}
    </div>
  );
}
