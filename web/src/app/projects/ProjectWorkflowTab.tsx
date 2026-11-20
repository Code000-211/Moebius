/**
 * The Workflow tab of a project's settings: the script of each column of its board, picked from a list, in the same
 * editor as the board's Script page. A column that runs the project's own script says so; Save commits it to the
 * harness configuration repository and "Use the built-in script" gives it up. Only an owner can save: the server
 * answers anyone else with its refusal, which the editor shows.
 */
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { projectScripts, script } from "../../api/calls/board";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import type { Project } from "../../api/types/projects";
import { Select } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { SkeletonLines } from "../../ui/page/Skeleton";
import { ScriptEditor, type ScriptNotice } from "../board/ScriptEditor";

export function ProjectWorkflowTab({ project }: { project: Project }) {
  const scripts = useQuery({
    queryKey: keys.projectScripts(project.id),
    queryFn: () => projectScripts(project.id),
  });
  const [column, setColumn] = useState<string | null>(null);
  const columns = scripts.data ?? [];
  const chosen = column ?? columns[0]?.column ?? "";
  return (
    <div>
      <div className="flex flex-col gap-4">
        <p className="m-0 max-w-prose text-base text-secondary">
          What each column does with a card. A column runs the built-in script until this project
          has one of its own.
        </p>
        {scripts.isPending && <SkeletonLines lines={6} message="Reading the scripts" />}
        {scripts.isError && (
          <Notice tone="error">
            {isApiError(scripts.error) ? scripts.error.message : "The scripts could not be read."}
          </Notice>
        )}
        {columns.length > 0 && (
          <>
            <Select
              aria-label="Column"
              className="w-64"
              options={columns.map((each): [string, string] => [
                each.column,
                each.source === "project" ? `${each.column} · its own` : each.column,
              ])}
              value={chosen}
              onChange={setColumn}
            />
            <Column key={chosen} project={project.id} column={chosen} />
          </>
        )}
      </div>
    </div>
  );
}

/** The editor of one column's script, started again when the script under it changes. */
function Column({ project, column }: { project: number; column: string }) {
  const source = useQuery({
    queryKey: keys.script(project, column),
    queryFn: () => script(project, column),
  });
  const [notice, setNotice] = useState<ScriptNotice | null>(null);
  if (source.isPending) return <SkeletonLines lines={8} message="Reading the script" />;
  if (source.isError)
    return (
      <Notice tone="error">
        {isApiError(source.error) ? source.error.message : "The script could not be read."}
      </Notice>
    );
  return (
    <ScriptEditor
      key={`${source.data.source}\n${source.data.text}`}
      project={project}
      column={column}
      path={source.data.path}
      own={source.data.source === "project"}
      saved={source.data.text}
      notice={notice}
      setNotice={setNotice}
    />
  );
}
