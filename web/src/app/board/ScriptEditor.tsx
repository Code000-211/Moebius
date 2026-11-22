/**
 * The editor of one column's script of a project (CodeMirror with Lua highlighting, loaded when it opens): where the
 * project keeps the script, whether it is its own or the built-in one, its text, and Save, Revert and "Use the
 * built-in script". The Columns tab of a project's settings draws it; with `readOnly` the text is shown and nothing can be changed.
 */
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { lazy, Suspense, useState } from "react";
import { deleteScript, saveScript } from "../../api/calls/board";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import { Button } from "../../ui/components/Button";
import { Notice as NoticeLine } from "../../ui/page/Notice";
import { SkeletonLines } from "../../ui/page/Skeleton";
import { type ScriptProblem, scriptProblem } from "./scriptError";

const CodeEditor = lazy(() => import("../../ui/CodeEditor"));

/** What came of the last save, for the line under the editor. */
export type ScriptNotice = { tone: "ok" | "error"; text: string };

export function ScriptEditor({
  project,
  column,
  path,
  own,
  saved,
  notice,
  setNotice,
  readOnly = false,
}: {
  project: number;
  column: string;
  path: string;
  own: boolean;
  saved: string;
  notice: ScriptNotice | null;
  setNotice: (notice: ScriptNotice | null) => void;
  /** Whoever reads the script is not allowed to change it. */
  readOnly?: boolean;
}) {
  const queryClient = useQueryClient();
  const [text, setText] = useState(saved);
  const [problem, setProblem] = useState<ScriptProblem | null>(null);
  const [asking, setAsking] = useState(false);
  const changed = text !== saved;

  const done = (detail: { text: string; source: string }, message: string) => {
    queryClient.setQueryData(keys.script(project, column), { ...detail, column, path });
    void queryClient.invalidateQueries({ queryKey: keys.scripts });
    setProblem(null);
    setNotice({ tone: "ok", text: message });
  };
  const failed = (error: unknown, what: string) => {
    const answer = isApiError(error) ? error.message : `Could not ${what}.`;
    // Only a refused script (422) is a problem of the text; the rest is a sentence for the person.
    if (isApiError(error) && error.status === 422) {
      setProblem(scriptProblem(answer));
      setNotice(null);
    } else {
      setNotice({ tone: "error", text: answer });
    }
  };

  const save = useMutation({
    mutationFn: () => saveScript(project, column, text),
    onSuccess: (detail) =>
      done(
        detail,
        detail.text === saved && !own
          ? "Nothing changed."
          : "Saved as a commit on the default branch.",
      ),
    onError: (error) => failed(error, "save the script"),
  });
  const useBuiltIn = useMutation({
    mutationFn: () => deleteScript(project, column),
    onSuccess: (detail) => {
      setAsking(false);
      done(detail, "The column runs the built-in script again.");
    },
    onError: (error) => {
      setAsking(false);
      failed(error, "remove the script");
    },
  });

  return (
    <div className="flex flex-col gap-4">
      <p className="m-0 text-sm text-secondary">
        <span className="font-mono">{path}</span> · {own ? "this project's own" : "built in"}
      </p>
      <Suspense fallback={<SkeletonLines lines={10} message="Opening the editor" />}>
        <CodeEditor
          label={`Script of ${column}`}
          value={text}
          readOnly={readOnly}
          problem={problem?.line != null ? { line: problem.line, message: problem.message } : null}
          onChange={(next) => {
            setText(next);
            setProblem(null);
            setNotice(null);
          }}
        />
      </Suspense>
      {problem && (
        <div role="alert" className="flex flex-col gap-2 text-base text-error-text">
          <p data-testid="script-problem" className="m-0">
            {problem.line === null ? problem.message : `Line ${problem.line}: ${problem.message}`}
          </p>
          {problem.full.includes("\n") && (
            <pre className="m-0 overflow-auto rounded-control bg-code p-4 font-mono text-sm text-text">
              {problem.full}
            </pre>
          )}
        </div>
      )}
      {notice && <NoticeLine tone={notice.tone}>{notice.text}</NoticeLine>}
      {!readOnly && (
        <div className="flex flex-wrap items-center gap-3">
          <Button disabled={save.isPending || (!changed && own)} onClick={() => save.mutate()}>
            Save
          </Button>
          <Button
            variant="secondary"
            disabled={!changed}
            onClick={() => {
              setText(saved);
              setProblem(null);
              setNotice(null);
            }}
          >
            Revert changes
          </Button>
          {own &&
            (asking ? (
              <span className="flex flex-wrap items-center gap-3 text-base text-text">
                Use the built-in script instead?
                <Button variant="secondary" onClick={() => setAsking(false)}>
                  Keep it
                </Button>
                <Button
                  variant="danger"
                  disabled={useBuiltIn.isPending}
                  onClick={() => useBuiltIn.mutate()}
                >
                  Use the built-in script
                </Button>
              </span>
            ) : (
              <Button variant="secondary" onClick={() => setAsking(true)}>
                Use the built-in script
              </Button>
            ))}
        </div>
      )}
    </div>
  );
}
