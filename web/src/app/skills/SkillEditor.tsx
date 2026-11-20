/**
 * The editor of one skill, in the editor window (`ui/components/EditorWindow.tsx`): its name while a new skill is
 * named, its description, the text of `SKILL.md`, and the list of its other files with "New file". Save and Cancel are
 * the window's; a draft is guarded there. Delete asks first, and a skill Moebius brings is read only, with "Update from
 * the repository" for one copied from a repository. A file opens in the same window (`FileEditor`) and "Back" returns to
 * the skill with what was typed kept. A save closes the window and says so in a toast.
 */
import { FileIcon } from "@phosphor-icons/react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Link } from "react-router";
import { startChat } from "../../api/calls/chat";
import { deleteSkill, saveSkill } from "../../api/calls/harness";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import type { SkillDetail, SkillSource } from "../../api/types/harness";
import { Button } from "../../ui/components/Button";
import { DeleteButton, EditorWindow } from "../../ui/components/EditorWindow";
import { Input } from "../../ui/components/Input";
import { ConfirmDelete } from "../../ui/page/ConfirmDelete";
import { Field } from "../../ui/page/Field";
import { MarkdownBody, type MarkdownView } from "../../ui/page/MarkdownBody";
import { Notice } from "../../ui/page/Notice";
import { StateLabel } from "../../ui/page/StateLabel";
import { useToast } from "../../ui/page/Toast";
import { updateRequest } from "../harness/importRequest";
import { FileEditor } from "./FileEditor";
import { skillRequest } from "./model";

export function SkillEditor({
  name: initialName,
  naming,
  builtIn,
  source,
  detail,
  onClose,
}: {
  name: string;
  /** A new skill is being written: its name is asked for. */
  naming: boolean;
  builtIn: boolean;
  source: SkillSource | undefined;
  detail?: SkillDetail;
  onClose: () => void;
}) {
  const queryClient = useQueryClient();
  const toast = useToast();
  const [name, setName] = useState(initialName);
  const [description, setDescription] = useState(detail?.description ?? "");
  const [text, setText] = useState(detail?.text ?? "");
  const [error, setError] = useState<string | null>(null);
  const [deleting, setDeleting] = useState(false);
  // The file open in the window instead of the skill: a path, or "" while a new one is named; null for the skill.
  const [file, setFile] = useState<string | null>(null);
  // How a Markdown text is read. It belongs to the skill rather than to the file open.
  const [view, setView] = useState<MarkdownView>("raw");

  const exists = detail !== undefined && !builtIn;
  const files = detail?.files ?? [];
  const dirty = naming
    ? name !== "" || description !== "" || text !== ""
    : description !== (detail?.description ?? "") || text !== (detail?.text ?? "");

  const save = useMutation({
    mutationFn: () => {
      const request = skillRequest({ name, description, text });
      if (typeof request === "string") throw new Error(request);
      return saveSkill(request.name, request.description, request.text);
    },
    onSuccess: async (saved) => {
      toast({
        kind: "confirmation",
        text: saved.commit ? `Committed: ${saved.commit}` : "Saved.",
      });
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: keys.skills }),
        queryClient.invalidateQueries({ queryKey: keys.skill(saved.name) }),
      ]);
      onClose();
    },
    onError: (failure) => setError(sayError(failure, "Could not save the skill. Try again.")),
  });
  const update = useMutation({
    mutationFn: () => {
      if (!source) throw new Error("This skill has no source.");
      return startChat({ agent: "Moebius", message: updateRequest(name, source) });
    },
    onError: (failure) => setError(sayError(failure, "Could not ask the agent. Try again.")),
  });

  if (file !== null) {
    return (
      <FileEditor
        skill={name}
        path={file}
        builtIn={builtIn}
        view={view}
        onViewChange={setView}
        onBack={() => setFile(null)}
      />
    );
  }

  return (
    <EditorWindow
      open
      title={naming ? "New skill" : builtIn ? name : `Edit skill ${name}`}
      dirty={dirty}
      saving={save.isPending}
      error={error}
      actions={
        exists ? (
          <DeleteButton onClick={() => setDeleting(true)}>Delete skill</DeleteButton>
        ) : undefined
      }
      onSave={() => {
        setError(null);
        save.mutate();
      }}
      onClose={onClose}
    >
      {naming && (
        <Field label="Name of the new skill">
          {(id) => (
            <Input
              id={id}
              mono
              autoFocus
              value={name}
              placeholder="pdf"
              onChange={(event) => {
                setName(event.target.value);
                setError(null);
              }}
            />
          )}
        </Field>
      )}
      {builtIn ? (
        <div className="flex flex-col gap-3">
          <div className="flex flex-wrap items-center gap-3">
            <StateLabel>Built in · read only</StateLabel>
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
          <p className="m-0 text-base text-secondary">{description}</p>
        </div>
      ) : (
        <Field label="Description">
          {(id) => (
            <Input
              id={id}
              value={description}
              placeholder="One line that says when to use the skill"
              onChange={(event) => {
                setDescription(event.target.value);
                setError(null);
              }}
            />
          )}
        </Field>
      )}
      {source && (
        <p className="m-0 break-words text-sm text-secondary">
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
      <div className="flex min-w-0 flex-col gap-2">
        <span className="font-mono text-sm font-semibold text-secondary">SKILL.md</span>
        <MarkdownBody
          label="SKILL.md"
          text={text}
          readOnly={builtIn}
          rows={7}
          view={view}
          onViewChange={setView}
          placeholder="What the skill teaches the agent, in Markdown."
          onChange={(next) => {
            setText(next);
            setError(null);
          }}
        />
      </div>
      {(files.length > 0 || (!builtIn && exists)) && (
        <div className="flex flex-col gap-2">
          <div className="flex items-center justify-between gap-3">
            <span className="text-sm font-semibold text-secondary">Files</span>
            {!builtIn && exists && (
              <Button variant="secondary" size="sm" onClick={() => setFile("")}>
                New file
              </Button>
            )}
          </div>
          {/* biome-ignore lint/a11y/noRedundantRoles: preflight sets list-style: none, and WebKit then drops a list with no marker from the accessibility tree entirely */}
          <ul role="list" className="m-0 flex list-none flex-col gap-1 p-0">
            {files.map((path) => (
              <li key={path}>
                <button
                  type="button"
                  onClick={() => setFile(path)}
                  className="flex min-h-8 w-full cursor-pointer items-center gap-3 rounded-control px-3 text-left font-mono text-sm text-secondary transition-colors duration-(--motion-routine) ease-out hover:bg-hover hover:text-text"
                >
                  <FileIcon className="size-4 shrink-0 text-secondary" />
                  <span className="min-w-0 break-all">{path}</span>
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}
      <ConfirmDelete
        open={deleting}
        onOpenChange={setDeleting}
        title="Delete this skill?"
        confirmLabel="Delete skill"
        onConfirm={async () => {
          await deleteSkill(name);
          await queryClient.invalidateQueries({ queryKey: keys.skills });
          onClose();
        }}
      >
        The skill {name} and its files are removed from the harness configuration repository.
      </ConfirmDelete>
    </EditorWindow>
  );
}
