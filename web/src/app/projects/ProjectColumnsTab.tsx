/**
 * The Columns tab of a project's settings: the script of each column of its board, picked from a list, in the one
 * editor of column scripts. The column is part of the address (`/projects/:projectId/settings/columns/:column`), which
 * is where the board's column menu leads. A column that runs the project's own script says so; Save commits it to the
 * harness configuration repository and "Use the built-in script" gives it up. Only an owner edits: anyone else reads
 * the script. The columns a project adds to the six built-in ones are defined in its `kanban.toml` in the harness
 * configuration repository, not here: they are listed read-only (name, the column each counts as, its script file) and
 * their scripts are edited in the same editor.
 */
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { useNavigate } from "react-router";
import { projectScripts, script } from "../../api/calls/board";
import { BUILT_IN, columnsOf } from "../../api/columns";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import type { Project } from "../../api/types/projects";
import { Select } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { SkeletonLines } from "../../ui/page/Skeleton";
import { ScriptEditor, type ScriptNotice } from "../board/ScriptEditor";
import { scriptPath } from "../pages";

export function ProjectColumnsTab({
  project,
  column,
  canEdit,
}: {
  project: Project;
  /** The column the address names, if it names one; else the first. */
  column?: string;
  canEdit: boolean;
}) {
  const navigate = useNavigate();
  const scripts = useQuery({
    queryKey: keys.projectScripts(project.id),
    queryFn: () => projectScripts(project.id),
  });
  const columns = scripts.data ?? [];
  // The columns the project adds to the built-in six, with the file of each one's script.
  const custom = columnsOf(project).filter(
    (each) => each.custom ?? !(BUILT_IN as readonly string[]).includes(each.name),
  );
  const chosen = columns.find((each) => each.column === column)?.column ?? columns[0]?.column ?? "";
  return (
    <div>
      <div className="flex flex-col gap-4">
        <p className="m-0 max-w-prose text-base text-secondary">
          What each column does with a card. A built-in column runs the built-in script until this
          project has one of its own. A column this project adds runs the project's own script for
          it; if that file is missing, it runs the script of the column it counts as.
        </p>
        <p className="m-0 max-w-prose text-sm text-muted">
          The columns are defined in <code>projects/{project.name}/kanban.toml</code> of the harness
          configuration repository.
        </p>
        {project.columns_problem && (
          <Notice tone="error">
            The columns of this project could not be read from its kanban.toml, so it has the
            built-in ones: {project.columns_problem}
          </Notice>
        )}
        {custom.length > 0 && (
          <ul
            aria-label="Custom columns"
            className="m-0 flex max-w-prose list-none flex-col divide-y divide-hairline rounded-lg border border-hairline p-0"
          >
            {custom.map((each) => (
              <li
                key={each.name}
                className="flex flex-wrap items-baseline gap-x-3 gap-y-1 px-3 py-2 text-base"
              >
                <span className="font-medium text-text">{each.name}</span>
                <span className="text-secondary">counts as {each.counts_as}</span>
                <code className="ml-auto text-sm text-muted">
                  {each.script ?? columns.find((one) => one.column === each.name)?.path ?? ""}
                </code>
              </li>
            ))}
          </ul>
        )}
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
              onChange={(next) => navigate(scriptPath(project.id, next))}
            />
            <Column
              key={chosen}
              project={project.id}
              column={chosen}
              counts={custom.find((each) => each.name === chosen)?.counts_as}
              canEdit={canEdit}
            />
          </>
        )}
      </div>
    </div>
  );
}

/** The editor of one column's script, started again when the script under it changes. */
function Column({
  project,
  column,
  counts,
  canEdit,
}: {
  project: number;
  column: string;
  /** The built-in column this column counts as, when the project defines the column itself. */
  counts?: string;
  canEdit: boolean;
}) {
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
      counts={counts}
      saved={source.data.text}
      notice={notice}
      setNotice={setNotice}
      readOnly={!canEdit}
    />
  );
}
