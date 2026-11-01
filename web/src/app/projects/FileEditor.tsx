/**
 * The editor of one file: its text in a field, saved as a commit on the default branch with Save or Ctrl+S. A file that
 * is binary or over 1 MB is said to be so and not edited. The text being edited belongs to the caller, so it stays when
 * another file is shown; the server's refusal of a save (a protected branch, a push that failed) is shown as it came.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef } from "react";
import { readFile, saveFile } from "../../api/calls/projects";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import { Button } from "../../ui/components/Button";
import { Loader } from "../../ui/components/Loader";
import { CONTROL } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { lineStart, notEditable } from "./model";

export function FileEditor({
  repository,
  path,
  draft,
  onDraft,
  line,
  onLineShown,
}: {
  repository: number;
  path: string;
  /** The text being edited, or undefined while the file is as the branch has it. */
  draft: string | undefined;
  onDraft: (text: string | undefined) => void;
  /** The line to put the cursor on, once, when the file is shown. */
  line: number | null;
  onLineShown: () => void;
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

  // Puts the cursor on the line a search hit named, once the text is there.
  // biome-ignore lint/correctness/useExhaustiveDependencies: it runs when a line is asked for and the file is read
  useEffect(() => {
    if (line === null || saved === undefined || !field.current) return;
    const at = lineStart(field.current.value, line);
    field.current.focus();
    field.current.setSelectionRange(at, at);
    const height = Number.parseFloat(getComputedStyle(field.current).lineHeight) || 18;
    field.current.scrollTop = Math.max(0, (line - 3) * height);
    onLineShown();
  }, [line, saved]);

  if (file.isPending) return <Loader message="Reading the file…" className="py-double" />;
  if (file.isError) {
    return <Notice tone="error">{file.error.message}</Notice>;
  }
  const refusal = notEditable(file.data);
  if (refusal) return <p className="m-0 text-[13px] text-normal">{refusal}</p>;

  const submit = () => {
    if (unsaved && !save.isPending) save.mutate(text);
  };
  return (
    <div className="flex min-w-0 flex-1 flex-col gap-base">
      <div className="flex flex-wrap items-center gap-base">
        <span className="min-w-0 flex-1 truncate font-mono text-[12px] text-low">{path}</span>
        {save.isPending ? (
          <span className="text-[12px] text-low">Saving.</span>
        ) : unsaved ? (
          <span className="text-[12px] text-amber">Unsaved changes.</span>
        ) : save.isSuccess ? (
          <span className="text-[12px] text-success">Committed to the default branch.</span>
        ) : null}
        <Button size="sm" disabled={!unsaved || save.isPending} onClick={submit}>
          Save
        </Button>
      </div>
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
        className={`${CONTROL} h-[420px] w-full resize-y whitespace-pre font-mono text-[12px] leading-5`}
        wrap="off"
      />
      {save.isError && (
        <Notice tone="error">
          {`The server refused: ${isApiError(save.error) ? save.error.message : "the file could not be saved."}`}
        </Notice>
      )}
    </div>
  );
}
