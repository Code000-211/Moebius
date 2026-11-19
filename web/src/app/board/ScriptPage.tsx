/**
 * The script a column runs for the cards of a project, in an editor (CodeMirror with Lua highlighting, loaded when this
 * page opens): where the project keeps it, whether it is the project's own or the built-in one, and its text. The
 * project is `?project=` (the dock links with the card's project), else the home project of the board's repository.
 * Save is `PUT /v1/projects/{id}/scripts/{column}`, which the server checks as `moebius check-script` does and
 * commits; a script it refuses is answered with the check's message, shown under the editor and marked at its line.
 * "Use the built-in script" (`DELETE`) gives up the project's own, after asking. The board's dock links here.
 */
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { Link, useParams, useSearchParams } from "react-router";
import { script, snapshot } from "../../api/calls/board";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import { Button } from "../../ui/components/Button";
import { Notice as NoticeLine } from "../../ui/page/Notice";
import { PageHeader } from "../../ui/page/PageHeader";
import { SkeletonLines } from "../../ui/page/Skeleton";
import { boardPath } from "../pages";
import { ScriptEditor, type ScriptNotice } from "./ScriptEditor";

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
  const [notice, setNotice] = useState<ScriptNotice | null>(null);
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
        <ScriptEditor
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
