/**
 * The editor of one file, saved as a commit on the default branch with Save or Ctrl+S. Its head row carries the tabs of
 * the files that are open, so the row above the text is where the file is chosen and where the save answers; the tab
 * names the file, so the path is not written out again under it. A file that is binary or over 1 MB is said to be so
 * and not edited. The text being edited belongs to the caller, so it stays when another file is shown; the server's
 * refusal of a save (a protected branch, a push that failed) says first that nothing was committed and keeps the text
 * as a draft.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { type ReactNode, useEffect, useRef } from "react";
import { readFile, saveFile } from "../../api/calls/projects";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import { Button } from "../../ui/components/Button";
import { Loader } from "../../ui/components/Loader";
import { cn } from "../../ui/lib/cn";
import { CONTROL } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { lineStart, notEditable } from "./model";

/** The column the editor and the word about its file stand in: the head row, then the file or what stands in for it. */
const COLUMN = "flex min-w-0 flex-1 flex-col gap-4";

export function FileEditor({
  repository,
  path,
  draft,
  onDraft,
  line,
  onLineShown,
  tabs,
}: {
  repository: number;
  path: string;
  /** The text being edited, or undefined while the file is as the branch has it. */
  draft: string | undefined;
  onDraft: (text: string | undefined) => void;
  /** The line to put the cursor on, once, when the file is shown. */
  line: number | null;
  onLineShown: () => void;
  /** The tabs of the files that are open, which sit on this editor's head row. */
  tabs: ReactNode;
}) {
  const queryClient = useQueryClient();
  const field = useRef<HTMLTextAreaElement>(null);
  const file = useQuery({
    queryKey: keys.file(repository, path),
    queryFn: () => readFile(repository, path),
  });
  const save = useMutation({
    mutationFn: (text: string) => saveFile(repository, path, text),
    onSuccess: (saved, text) => {
      queryClient.setQueryData(keys.file(repository, path), saved);
      // What was typed since the request went out stays a draft.
      onDraft(field.current?.value === text ? undefined : field.current?.value);
    },
  });
  const saved = file.data?.text;
  const text = draft ?? saved ?? "";
  const unsaved = saved !== undefined && text !== saved;

  /** The row above the text: the file is chosen on its left, and the save answers on its right. */
  const head = (right?: ReactNode) => (
    <div className="flex min-w-0 flex-wrap items-center justify-between gap-4">
      {tabs}
      {right}
    </div>
  );

  // Puts the cursor on the line a search hit named, once the text is there.
  // biome-ignore lint/correctness/useExhaustiveDependencies: it runs when a line is asked for and the file is read
  useEffect(() => {
    if (line === null || saved === undefined || !field.current) return;
    const at = lineStart(field.current.value, line);
    field.current.focus();
    field.current.setSelectionRange(at, at);
    const height = Number.parseFloat(getComputedStyle(field.current).lineHeight) || 20;
    field.current.scrollTop = Math.max(0, (line - 3) * height);
    onLineShown();
  }, [line, saved]);

  if (file.isPending) return <Loader message="Reading the file" className="py-8" />;
  if (file.isError) {
    return (
      <div className={COLUMN}>
        {head()}
        <Notice tone="error">
          <span className="font-semibold">Couldn't read the file. </span>
          <span className="break-words">{file.error.message}</span>
        </Notice>
      </div>
    );
  }
  const refusal = notEditable(file.data);
  if (refusal)
    return (
      <div className={COLUMN}>
        {head()}
        <p className="m-0 break-words text-base text-secondary">{refusal}</p>
      </div>
    );

  const submit = () => {
    if (unsaved && !save.isPending) save.mutate(text);
  };
  return (
    <div className={COLUMN}>
      {head(
        <div className="flex items-center gap-4">
          {save.isPending && (
            <span className="text-sm text-secondary" role="status">
              Saving
            </span>
          )}
          {!save.isPending && unsaved && (
            <span className="text-sm text-attention-text" role="status">
              Unsaved changes.
            </span>
          )}
          {!save.isPending && !unsaved && save.isSuccess && (
            <span className="text-sm text-complete-text" role="status">
              Committed to the default branch.
            </span>
          )}
          <Button size="sm" disabled={!unsaved || save.isPending} onClick={submit}>
            Save
          </Button>
        </div>,
      )}
      <textarea
        ref={field}
        aria-label={`Text of ${path}`}
        spellCheck={false}
        value={text}
        onChange={(e) => {
          save.reset();
          onDraft(e.target.value === saved ? undefined : e.target.value);
        }}
        onKeyDown={(e) => {
          if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
            e.preventDefault();
            submit();
          }
        }}
        className={cn(CONTROL, "h-96 resize-y whitespace-pre font-mono text-sm leading-6")}
        wrap="off"
      />
      {save.isError && (
        <Notice tone="error">
          <span className="font-semibold">Nothing was committed. Your text stays as a draft. </span>
          {`The server refused: ${isApiError(save.error) ? save.error.message : "the file could not be saved."}`}
        </Notice>
      )}
    </div>
  );
}
