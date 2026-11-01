/**
 * The editor of one skill: the head (Save and Delete; "Built in · read only" for a skill Moebius brings, with
 * "Update from upstream" for one copied from a repository), its description, the list of its files with "New file", and
 * the text of `SKILL.md` or the editor of the file open (`FileEditor`).
 */
import { FileIcon } from "@phosphor-icons/react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Link } from "react-router";
import { startChat } from "../../api/calls/chat";
import { deleteSkill, saveSkill } from "../../api/calls/harness";
import { keys } from "../../api/keys";
import type { SkillDetail, SkillSource } from "../../api/types/harness";
import { Tag } from "../../ui/chat/Tag";
import { Button } from "../../ui/components/Button";
import { cn } from "../../ui/lib/cn";
import { Box, Committed, EditorHead, Field, Notice, TextArea, TextInput } from "../harness/form";
import { updateRequest } from "../harness/importRequest";
import { FileEditor } from "./FileEditor";
import { skillRequest } from "./model";

export function SkillEditor({
  name: initialName,
  naming: initialNaming,
  builtIn,
  source,
  detail,
  saved,
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
  onNamed: (name: string, commit: string | null) => void;
  onDeleted: () => void;
}) {
  const queryClient = useQueryClient();
  const [name, setName] = useState(initialName);
  const [naming, setNaming] = useState(initialNaming);
  const [exists, setExists] = useState(detail !== undefined && !builtIn);
  const [description, setDescription] = useState(detail?.description ?? "");
  const [text, setText] = useState(detail?.text ?? "");
  const [files, setFiles] = useState(detail?.files ?? []);
  // The other file open; null while SKILL.md is, "" for a new file being named.
  const [openFile, setOpenFile] = useState<string | null>(null);
  const [newFile, setNewFile] = useState(false);
  // What the last save of a file made: its commit (null for none); undefined while nothing was saved.
  const [fileSaved, setFileSaved] = useState<string | null | undefined>(undefined);
  const [notice, setNotice] = useState<{ error: string } | { commit: string | null } | null>(
    saved !== undefined ? { commit: saved } : null,
  );

  const edit =
    <T,>(set: (value: T) => void) =>
    (value: T) => {
      set(value);
      setNotice(null);
    };
  const save = useMutation({
    mutationFn: () => {
      const request = skillRequest({ name, description, text });
      if (typeof request === "string") throw new Error(request);
      return saveSkill(request.name, request.description, request.text);
    },
    onSuccess: async (saved) => {
      setName(saved.name);
      setNaming(false);
      setExists(true);
      setFiles(saved.files);
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
    <Box>
      <EditorHead
        name={name}
        naming={naming}
        exists={exists}
        deleteLabel="Delete skill"
        namePlaceholder="Name of the new skill"
        onName={edit(setName)}
        saving={save.isPending}
        canSave
        onSave={() => save.mutate()}
        onDelete={() => remove.mutate()}
        deleting={remove.isPending}
        readOnly={
          builtIn ? (
            <div className="flex items-center gap-plusfifty">
              <Tag>Built in · read only</Tag>
              {source && (
                <Button
                  variant="outline"
                  size="sm"
                  disabled={update.isPending || update.isSuccess}
                  onClick={() => update.mutate()}
                >
                  {update.isPending ? "Asking…" : "Update from upstream"}
                </Button>
              )}
            </div>
          ) : undefined
        }
      />
      {builtIn ? (
        <p className="m-0 text-sm text-normal">{description}</p>
      ) : (
        <Field label="Description">
          {(id) => (
            <TextInput
              id={id}
              value={description}
              placeholder="One line that says when to use the skill"
              onChange={(event) => edit(setDescription)(event.target.value)}
            />
          )}
        </Field>
      )}
      {source && (
        <>
          <span className="font-mono text-[11.5px] text-low">
            Copied from {source.url} at commit {source.commit.slice(0, 12)}
          </span>
          <span className="font-mono text-[11.5px] text-low">
            A skill of this name in the harness repository replaces it.
          </span>
        </>
      )}
      {update.isSuccess && (
        <div className="flex items-center gap-base">
          <Notice tone="ok">The Moebius agent has the request.</Notice>
          <Button asChild variant="outline" size="sm">
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
    <div className="flex flex-col gap-double">
      {head}
      <Box
        title="Files"
        aside={builtIn ? "Read only" : !exists ? "Save the skill to add files" : undefined}
        actions={
          !builtIn && exists ? (
            <Button
              variant="outline"
              size="xs"
              onClick={() => {
                setOpenFile(null);
                setNewFile(true);
                setFileSaved(undefined);
              }}
            >
              New file
            </Button>
          ) : undefined
        }
      >
        <ul className="m-0 flex list-none flex-col gap-half p-0">
          {["SKILL.md", ...files].map((path) => {
            const open = path === "SKILL.md" ? !editing : openFile === path;
            return (
              <li key={path}>
                <button
                  type="button"
                  aria-current={open ? "true" : undefined}
                  onClick={() => {
                    setNewFile(false);
                    setOpenFile(path === "SKILL.md" ? null : path);
                    setFileSaved(undefined);
                  }}
                  className={cn(
                    "flex w-full items-center gap-base rounded-sm px-2 py-1 text-left font-mono text-xs text-high hover:bg-lane",
                    open && "bg-lane",
                  )}
                >
                  <FileIcon className="size-icon-sm text-normal" />
                  {path}
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
          onSaved={(saved, path) => {
            setFiles(saved.files);
            setNewFile(false);
            setOpenFile(path);
            setFileSaved(saved.commit ?? null);
          }}
          onDeleted={(saved) => {
            setFiles(saved.files);
            setNewFile(false);
            setOpenFile(null);
            setNotice({ commit: saved.commit ?? null });
          }}
        />
      ) : (
        <Box title="SKILL.md" aside="Markdown · the text after the frontmatter">
          <TextArea
            rows={18}
            aria-label="SKILL.md"
            value={text}
            readOnly={builtIn}
            placeholder="What the skill teaches the agent, in Markdown."
            onChange={(event) => edit(setText)(event.target.value)}
          />
        </Box>
      )}
    </div>
  );
}
