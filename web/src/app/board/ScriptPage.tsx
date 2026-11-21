/**
 * The script a column runs for the cards of a project, in an editor (CodeMirror with Lua highlighting, loaded when this
 * page opens): where the project keeps it, whether it is the project's own or the built-in one, and its text. The
 * project is the one in the address, `/projects/:projectId/settings/columns/:column`.
 * Save is `PUT /v1/projects/{id}/scripts/{column}`, which the server checks as `moebius check-script` does and
 * commits; a script it refuses is answered with the check's message, shown under the editor and marked at its line.
 * "Use the built-in script" (`DELETE`) gives up the project's own, after asking. The board's dock links here.
 */
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { Link, useParams, useSearchParams } from "react-router";
import { script } from "../../api/calls/board";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import { Button } from "../../ui/components/Button";
import { Notice as NoticeLine } from "../../ui/page/Notice";
import { PageHeader } from "../../ui/page/PageHeader";
import { SkeletonLines } from "../../ui/page/Skeleton";
import { boardPath } from "../pages";
import { ScriptEditor, type ScriptNotice } from "./ScriptEditor";

export function ScriptPage() {
  const { projectId, column } = useParams();
  const [search] = useSearchParams();
  const project = Number(projectId);
  const name = column ?? "";
  // The board the dock came from: `?repository=` names it, else the project's own board.
  const repository = Number(search.get("repository")) || undefined;
  const source = useQuery({
    queryKey: keys.script(project, name),
    queryFn: () => script(project, name),
    enabled: Number.isInteger(project),
  });
  // What came of the last save lives here: the editor below starts again when the script it shows changes.
  const [notice, setNotice] = useState<ScriptNotice | null>(null);
  return (
    <>
      <PageHeader
        title={`Script of ${name}`}
        actions={
          <Button asChild variant="ghost">
            <Link to={boardPath(project, repository)}>Back to the board</Link>
          </Button>
        }
      />
      {source.isPending && <SkeletonLines lines={8} message="Reading the script" />}
      {source.error && (
        <NoticeLine tone="error">
          {isApiError(source.error)
            ? source.error.message
            : "Could not read the script. Try again."}
        </NoticeLine>
      )}
      {source.data && (
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
