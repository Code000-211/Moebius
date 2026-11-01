/**
 * The editor of one of a skill's other files (the desktop app's `skills/files`): reading is
 * `GET /v1/skills/{name}/files/{path}`; saving and deleting are `PUT` and `DELETE` of the same route, each a commit in
 * the harness configuration repository. A built-in skill's files are read only, and a file that is not text is listed
 * with its size and can only be deleted.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { deleteSkillFile, saveSkillFile, skillFile } from "../../api/calls/harness";
import { keys } from "../../api/keys";
import type { SkillDetail } from "../../api/types/harness";
import { Button } from "../../ui/components/Button";
import { Box, Committed, Notice, TextArea, TextInput } from "../harness/form";
import { fileRequest, size } from "./model";

export function FileEditor({
  skill,
  path: initialPath,
  naming,
  builtIn,
  saved,
  onSaved,
  onDeleted,
}: {
  skill: string;
  path: string;
  naming: boolean;
  builtIn: boolean;
  /** The commit the save that opened this file made (null for none); undefined when it was not just saved. */
  saved?: string | null;
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
        <p className="m-0 text-sm text-normal">Reading the file…</p>
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
  return (
    <Box>
      <div className="flex flex-wrap items-center justify-between gap-base">
        {naming ? (
          <TextInput
            mono
            autoFocus
            aria-label="Path in the skill"
            value={path}
            placeholder="Path in the skill, such as scripts/fill.py"
            onChange={(event) => {
              setPath(event.target.value);
              setNotice(null);
            }}
            className="max-w-sm"
          />
        ) : (
          <h3 className="m-0 font-mono text-[14px] font-semibold text-high">{path}</h3>
        )}
        {builtIn ? (
          <span className="font-mono text-[11.5px] text-low">Read only</span>
        ) : asking ? (
          <div className="flex items-center gap-base">
            <span className="text-xs text-normal">Delete {path}?</span>
            <Button variant="outline" size="sm" onClick={() => setAsking(false)}>
              Keep
            </Button>
            <Button variant="destructive" size="sm" onClick={() => remove.mutate()}>
              Delete the file
            </Button>
          </div>
        ) : (
          <div className="flex items-center gap-base">
            {!naming && (
              <Button variant="outline" size="sm" onClick={() => setAsking(true)}>
                Delete file
              </Button>
            )}
            <Button
              size="sm"
              disabled={save.isPending || text === null}
              onClick={() => save.mutate()}
            >
              {save.isPending ? "Saving…" : "Save file"}
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
        <p className="m-0 text-sm text-normal">Not text, or too large to edit ({size(bytes)}).</p>
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
