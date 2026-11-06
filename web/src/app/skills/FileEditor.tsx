/**
 * The editor of one of a skill's other files (the former desktop app's `skills/files`): reading is
 * `GET /v1/skills/{name}/files/{path}`; saving and deleting are `PUT` and `DELETE` of the same route, each a commit in
 * the harness configuration repository. A built-in skill's files are read only, and a file that is not text is listed
 * with its size and can only be deleted. Save says what it knows — that a change is waiting, or why there is nothing
 * to save — and a file with nothing waiting is simply disabled. How the text is read is held by the skill, so moving
 * along its files keeps it.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { deleteSkillFile, saveSkillFile, skillFile } from "../../api/calls/harness";
import { keys } from "../../api/keys";
import type { SkillDetail } from "../../api/types/harness";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { Box } from "../../ui/page/Box";
import { Field, TextArea } from "../../ui/page/Field";
import { MarkdownBody, type MarkdownView } from "../../ui/page/MarkdownBody";
import { Notice } from "../../ui/page/Notice";
import { Status } from "../../ui/page/Status";
import { Committed } from "../harness/form";

import { fileRequest, size } from "./model";

/** Whether a file of a skill is Markdown, and so worth showing drawn as well as raw. */
const isMarkdown = (path: string) => path.trim().toLowerCase().endsWith(".md");

export function FileEditor({
  skill,
  path: initialPath,
  naming,
  builtIn,
  saved,
  view,
  onViewChange,
  onSaved,
  onDeleted,
}: {
  skill: string;
  path: string;
  naming: boolean;
  builtIn: boolean;
  /** The commit the save that opened this file made (null for none); undefined when it was not just saved. */
  saved?: string | null;
  /** How Markdown is read here, held by the skill so the next file opens the same way. */
  view: MarkdownView;
  onViewChange: (view: MarkdownView) => void;
  onSaved: (skill: SkillDetail, path: string) => void;
  onDeleted: (skill: SkillDetail) => void;
}) {
  const queryClient = useQueryClient();
  const detail = useQuery({
    queryKey: keys.skillFile(skill, initialPath),
    queryFn: () => skillFile(skill, initialPath),
    enabled: !naming,
    retry: false,
  });
  if (!naming && detail.isPending) {
    return (
      <Box title={initialPath}>
        <p className="m-0 text-base text-secondary">Reading the file</p>
      </Box>
    );
  }
  return (
    <Body
      key={initialPath}
      skill={skill}
      initialPath={initialPath}
      naming={naming}
      builtIn={builtIn}
      saved={saved}
      view={view}
      onViewChange={onViewChange}
      text={detail.data?.text ?? (naming ? "" : null)}
      size={detail.data?.size ?? 0}
      readError={detail.isError ? detail.error.message : null}
      onSaved={(saved, path) => {
        void queryClient.invalidateQueries({ queryKey: keys.skill(skill) });
        void queryClient.invalidateQueries({ queryKey: keys.skillFile(skill, path) });
        onSaved(saved, path);
      }}
      onDeleted={(saved) => {
        void queryClient.invalidateQueries({ queryKey: keys.skill(skill) });
        onDeleted(saved);
      }}
    />
  );
}

function Body({
  skill,
  initialPath,
  naming,
  builtIn,
  saved,
  view,
  onViewChange,
  text: initialText,
  size: bytes,
  readError,
  onSaved,
  onDeleted,
}: {
  skill: string;
  initialPath: string;
  naming: boolean;
  builtIn: boolean;
  saved: string | null | undefined;
  view: MarkdownView;
  onViewChange: (view: MarkdownView) => void;
  text: string | null;
  size: number;
  readError: string | null;
  onSaved: (skill: SkillDetail, path: string) => void;
  onDeleted: (skill: SkillDetail) => void;
}) {
  const [path, setPath] = useState(initialPath);
  const [text, setText] = useState(initialText);
  const [asking, setAsking] = useState(false);
  const [notice, setNotice] = useState<{ error: string } | { commit: string | null } | null>(
    readError ? { error: readError } : saved !== undefined ? { commit: saved } : null,
  );
  const dirty = text !== initialText || path !== initialPath;
  const editable = text !== null;
  const save = useMutation({
    mutationFn: () => {
      const request = fileRequest(path, text);
      if (typeof request === "string") throw new Error(request);
      return saveSkillFile(skill, request.path, request.text).then((saved) => ({
        saved,
        path: request.path,
      }));
    },
    onSuccess: ({ saved, path: savedPath }) => {
      setNotice({ commit: saved.commit ?? null });
      onSaved(saved, savedPath);
    },
    onError: (error) => setNotice({ error: error.message }),
  });
  const remove = useMutation({
    mutationFn: () => deleteSkillFile(skill, path),
    onSuccess: onDeleted,
    onError: (error) => setNotice({ error: error.message }),
  });

  // Why Save cannot be used, in words. A missing path is said by the field itself, so it is not repeated under the
  // button, and there being nothing to write is said by the disabled button itself.
  const missingPath = path.trim() === "";
  const needPath = "Enter the file's path inside the skill.";
  const why = !editable
    ? "Not a text file, so there is nothing to save"
    : missingPath
      ? needPath
      : undefined;
  const shown = missingPath ? undefined : why;
  const blocked = why !== undefined || !dirty;
  const label = shown ?? (dirty ? "Unsaved changes" : undefined);

  return (
    <Box className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center justify-between gap-4">
        {naming ? (
          <Field
            label="Path in the skill"
            hint="Relative to the skill, such as scripts/fill.py."
            error={missingPath ? needPath : undefined}
            className="min-w-0 max-w-sm flex-1"
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
                  setNotice(null);
                }}
              />
            )}
          </Field>
        ) : (
          <h3 className="m-0 min-w-0 break-all font-mono text-sm font-semibold text-text">
            {path}
          </h3>
        )}
        {builtIn ? (
          <Status>Read only</Status>
        ) : asking ? (
          <div className="flex flex-wrap items-center gap-3">
            <span className="text-base text-text">Delete {path}?</span>
            <Button variant="secondary" size="sm" onClick={() => setAsking(false)}>
              Keep
            </Button>
            <Button
              variant="danger"
              size="sm"
              disabled={remove.isPending}
              onClick={() => {
                setAsking(false);
                remove.mutate();
              }}
            >
              Delete the file
            </Button>
          </div>
        ) : (
          <div className="flex flex-wrap items-center justify-end gap-3">
            {!naming && (
              <Button variant="danger" size="sm" onClick={() => setAsking(true)}>
                Delete file
              </Button>
            )}
            {label && <span className="text-sm text-secondary">{label}</span>}
            <Button
              size="sm"
              disabled={save.isPending || blocked}
              title={blocked ? why : undefined}
              onClick={() => save.mutate()}
            >
              {save.isPending ? "Saving" : "Save file"}
            </Button>
          </div>
        )}
      </div>
      {notice &&
        ("error" in notice ? (
          <Notice tone="error">{notice.error}</Notice>
        ) : (
          <Committed commit={notice.commit} />
        ))}
      {text === null ? (
        <p className="m-0 text-base text-secondary">
          Not text, or too large to edit ({size(bytes)}).
        </p>
      ) : isMarkdown(path) ? (
        <MarkdownBody
          label="File text"
          text={text}
          readOnly={builtIn}
          view={view}
          onViewChange={onViewChange}
          placeholder="The file's text."
          onChange={(next) => {
            setText(next);
            setNotice(null);
          }}
        />
      ) : (
        <TextArea
          rows={18}
          aria-label="File text"
          value={text}
          readOnly={builtIn}
          placeholder="The file's text."
          onChange={(event) => {
            setText(event.target.value);
            setNotice(null);
          }}
        />
      )}
    </Box>
  );
}
