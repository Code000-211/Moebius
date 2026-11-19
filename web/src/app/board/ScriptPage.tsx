/**
 * The script a column runs for the cards of a project, in an editor (CodeMirror with Lua highlighting, loaded when this
 * page opens): where the project keeps it, whether it is the project's own or the built-in one, and its text. The
 * project is `?project=` (the dock links with the card's project), else the home project of the board's repository.
 * Save is `PUT /v1/projects/{id}/scripts/{column}`, which the server checks as `moebius check-script` does and
 * commits; a script it refuses is answered with the check's message, shown under the editor and marked at its line.
 * "Use the built-in script" (`DELETE`) gives up the project's own, after asking. The board's dock links here.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { lazy, Suspense, useState } from "react";
import { Link, useParams, useSearchParams } from "react-router";
import { deleteScript, saveScript, script, snapshot } from "../../api/calls/board";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import { Button } from "../../ui/components/Button";
import { Notice as NoticeLine } from "../../ui/page/Notice";
import { PageHeader } from "../../ui/page/PageHeader";
import { SkeletonLines } from "../../ui/page/Skeleton";
import { boardPath } from "../pages";
import { type ScriptProblem, scriptProblem } from "./scriptError";

const CodeEditor = lazy(() => import("../../ui/CodeEditor"));

export function ScriptPage() {
  const { id, column } = useParams();
  const [search] = useSearchParams();
  const repository = Number(id);
  const name = column ?? "";
  const named = search.get("project");
  const board = useQuery({
    queryKey: keys.snapshot,
    queryFn: () => snapshot(),
    enabled: named === null,
  });
  const home = board.data?.boards.find((each) => each.repository.id === repository)?.repository
    .home_project_id;
  const project = named === null ? home : Number(named);
  const missing = named === null && board.isSuccess && home === undefined;
  const source = useQuery({
    queryKey: keys.script(project ?? 0, name),
    queryFn: () => script(project ?? 0, name),
    enabled: project !== undefined,
  });
  // What came of the last save lives here: the editor below starts again when the script it shows changes.
  const [notice, setNotice] = useState<Notice | null>(null);
  return (
    <>
      <PageHeader
        title={`Script of ${name}`}
        context="What the column does with a card: when it starts a run, when it waits, and when it moves on."
        actions={
          <Button asChild variant="ghost">
            <Link to={boardPath(repository)}>Back to the board</Link>
          </Button>
        }
      />
      {source.isPending && !board.isError && !missing && (
        <SkeletonLines lines={8} message="Reading the script" />
      )}
      {missing && (
        <NoticeLine tone="error">This server has no repository with that address.</NoticeLine>
      )}
      {(source.error ?? board.error) && (
        <NoticeLine tone="error">
          {isApiError(source.error ?? board.error)
            ? (source.error ?? board.error)?.message
            : "Could not read the script. Try again."}
        </NoticeLine>
      )}
      {source.data && project !== undefined && (
        // The editor starts again from the script the server has whenever that changes under it.
        <Editor
          key={`${source.data.source}\n${source.data.text}`}
          project={project}
          column={name}
          path={source.data.path}
          own={source.data.source === "project"}
          saved={source.data.text}
          notice={notice}
          setNotice={setNotice}
        />
      )}
    </>
  );
}

type Notice = { tone: "ok" | "error"; text: string };

function Editor({
  project,
  column,
  path,
  own,
  saved,
  notice,
  setNotice,
}: {
  project: number;
  column: string;
  path: string;
  own: boolean;
  saved: string;
  notice: Notice | null;
  setNotice: (notice: Notice | null) => void;
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
    </div>
  );
}
