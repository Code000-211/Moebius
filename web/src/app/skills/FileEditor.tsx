/**
 * One of a skill's other files, in the editor window of the skill: reading is `GET /v1/skills/{name}/files/{path}`;
 * saving and deleting are `PUT` and `DELETE` of the same route, each a commit in the harness configuration repository.
 * The window is guarded like every editor (Cancel or Escape with an unsaved change asks first) and "Back" to the skill
 * is its close. A built-in skill's files are read only, and a file that is not text is listed with its size and can only
 * be deleted. How Markdown is read is held by the skill, so moving along its files keeps it.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { deleteSkillFile, saveSkillFile, skillFile } from "../../api/calls/harness";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import { DeleteButton, EditorWindow } from "../../ui/components/EditorWindow";
import { Input } from "../../ui/components/Input";
import { ConfirmDelete } from "../../ui/page/ConfirmDelete";
import { Field, TextArea } from "../../ui/page/Field";
import { MarkdownBody, type MarkdownView } from "../../ui/page/MarkdownBody";
import { useToast } from "../../ui/page/Toast";
import { fileRequest, size } from "./model";

/** Whether a file of a skill is Markdown, and so worth showing drawn as well as raw. */
const isMarkdown = (path: string) => path.trim().toLowerCase().endsWith(".md");

const NEED_PATH = "Enter the file's path inside the skill.";

/** `path` is empty while a new file is named. */
export function FileEditor({
  skill,
  path: initialPath,
  builtIn,
  view,
  onViewChange,
  onBack,
}: {
  skill: string;
  path: string;
  builtIn: boolean;
  /** How Markdown is read here, held by the skill so the next file opens the same way. */
  view: MarkdownView;
  onViewChange: (view: MarkdownView) => void;
  /** Back to the skill's own window. */
  onBack: () => void;
}) {
  const naming = initialPath === "";
  const queryClient = useQueryClient();
  const toast = useToast();
  const detail = useQuery({
    queryKey: keys.skillFile(skill, initialPath),
    queryFn: () => skillFile(skill, initialPath),
    enabled: !naming,
    retry: false,
  });
  const initialText = detail.data?.text ?? (naming ? "" : null);
  const [path, setPath] = useState(initialPath);
  // What was typed; undefined until something is, so the text read after the window opened is shown.
  const [typed, setTyped] = useState<string | undefined>(undefined);
  const [error, setError] = useState<string | null>(null);
  const [deleting, setDeleting] = useState(false);
  const text = typed ?? initialText;
  const dirty = typed !== undefined && typed !== initialText;
  const pathChanged = path !== initialPath;
  const missingPath = path.trim() === "";
  const refresh = (changed: string) =>
    Promise.all([
      queryClient.invalidateQueries({ queryKey: keys.skill(skill) }),
      queryClient.invalidateQueries({ queryKey: keys.skillFile(skill, changed) }),
    ]);

  const save = useMutation({
    mutationFn: () => {
      const request = fileRequest(path, text);
      if (typeof request === "string") throw new Error(request);
      return saveSkillFile(skill, request.path, request.text).then((saved) => ({
        saved,
        path: request.path,
      }));
    },
    onSuccess: async ({ saved, path: savedPath }) => {
      toast({
        kind: "confirmation",
        text: saved.commit ? `Committed: ${saved.commit}` : "Saved.",
      });
      await refresh(savedPath);
      onBack();
    },
    onError: (failure) => setError(sayError(failure, "Could not save the file. Try again.")),
  });

  const title = naming ? "New file" : initialPath;
  const loading = !naming && detail.isPending;
  return (
    <EditorWindow
      open
      title={`${skill} · ${title}`}
      dirty={dirty || (pathChanged && !missingPath)}
      saving={save.isPending}
      canSave={!builtIn && text !== null && !missingPath}
      error={error ?? (detail.isError ? detail.error.message : null)}
      actions={
        !naming && !builtIn ? (
          <DeleteButton onClick={() => setDeleting(true)}>Delete file</DeleteButton>
        ) : undefined
      }
      onSave={() => {
        setError(null);
        save.mutate();
      }}
      onClose={onBack}
    >
      {naming && (
        <Field
          label="Path in the skill"
          hint="Relative to the skill, such as scripts/fill.py."
          error={missingPath ? NEED_PATH : undefined}
        >
          {(id) => (
            <Input
              id={id}
              mono
              autoFocus
              value={path}
              placeholder="scripts/fill.py"
              onChange={(event) => {
                setPath(event.target.value);
                setError(null);
              }}
            />
          )}
        </Field>
      )}
      {loading ? (
        <p className="m-0 text-base text-secondary">Reading the file</p>
      ) : text === null ? (
        <p className="m-0 text-base text-secondary">
          Not text, or too large to edit ({size(detail.data?.size ?? 0)}).
        </p>
      ) : isMarkdown(path) ? (
        <MarkdownBody
          label="File text"
          text={text}
          readOnly={builtIn}
          rows={10}
          view={view}
          onViewChange={onViewChange}
          placeholder="The file's text."
          onChange={(next) => {
            setTyped(next);
            setError(null);
          }}
        />
      ) : (
        <TextArea
          rows={10}
          aria-label="File text"
          className="font-mono text-sm"
          value={text}
          readOnly={builtIn}
          placeholder="The file's text."
          onChange={(event) => {
            setTyped(event.target.value);
            setError(null);
          }}
        />
      )}
      <ConfirmDelete
        open={deleting}
        onOpenChange={setDeleting}
        title="Delete this file?"
        confirmLabel="Delete the file"
        onConfirm={async () => {
          const saved = await deleteSkillFile(skill, path);
          toast({
            kind: "confirmation",
            text: saved.commit ? `Committed: ${saved.commit}` : "Saved.",
          });
          await refresh(path);
          onBack();
        }}
      >
        The file {path} is removed from the skill {skill}.
      </ConfirmDelete>
    </EditorWindow>
  );
}
