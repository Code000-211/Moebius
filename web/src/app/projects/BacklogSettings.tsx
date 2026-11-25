/**
 * The "Backlog" section of a project's Repositories settings (ADR 0032): which file of which repository holds the
 * project's backlog, how an item is found in it, how many issues one sync makes at most, and the names of the labels a
 * stated difficulty and "lane required" give. "Preview" says what a sync would do and "Sync now" does it, each with
 * the report under the buttons. A nightly sync is a workflow (see the guide). An owner edits; anyone else reads.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { editBacklog, readBacklog, syncBacklog } from "../../api/calls/factory";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import type { Backlog, SyncReport } from "../../api/types/factory";
import type { Project } from "../../api/types/projects";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { Field, Select } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { SkeletonLines } from "../../ui/page/Skeleton";

const DEFAULT_CAP = 15;
const MOST_CAP = 100;

export function BacklogSettings({ project, canEdit }: { project: Project; canEdit: boolean }) {
  const backlog = useQuery({
    queryKey: keys.backlog(project.id),
    queryFn: () => readBacklog(project.id),
  });
  return (
    <section
      data-testid="backlog-settings"
      aria-labelledby="backlog-title"
      className="mt-8 flex max-w-xl flex-col gap-4"
    >
      <div className="flex flex-col gap-1">
        <h3 id="backlog-title" className="m-0 text-md font-semibold text-text">
          Backlog
        </h3>
        <p className="m-0 text-sm text-muted">
          A file of the project's items (headings with an ID such as K-47) that becomes issues, a
          few at a time, and follows the file when an item changes or closes.
        </p>
      </div>
      {backlog.isError ? (
        <Notice tone="error">The backlog could not be read. Try again.</Notice>
      ) : backlog.data === undefined ? (
        <SkeletonLines lines={3} />
      ) : (
        <Form
          key={JSON.stringify(backlog.data)}
          project={project}
          stored={backlog.data}
          canEdit={canEdit}
        />
      )}
    </section>
  );
}

function Form({
  project,
  stored,
  canEdit,
}: {
  project: Project;
  stored: Backlog | null;
  canEdit: boolean;
}) {
  const queryClient = useQueryClient();
  const names = project.repositories.map((each) => each.name);
  const [repository, setRepository] = useState(stored?.repository ?? names[0] ?? "");
  const [path, setPath] = useState(stored?.path ?? "");
  const [cap, setCap] = useState(String(stored?.cap ?? DEFAULT_CAP));
  const [marker, setMarker] = useState(stored?.closed_marker ?? "[closed]");
  const [difficulty, setDifficulty] = useState(stored?.difficulty_label ?? "difficulty:");
  const [lane, setLane] = useState(stored?.lane_label ?? "lane-required");
  const [pattern, setPattern] = useState(stored?.pattern ?? "");
  const [report, setReport] = useState<SyncReport | null>(null);
  const capNumber = Number(cap);
  const capOk = Number.isInteger(capNumber) && capNumber >= 1 && capNumber <= MOST_CAP;
  const valid = repository !== "" && path.trim() !== "" && capOk;
  const changed =
    stored === null ||
    repository !== stored.repository ||
    path.trim() !== stored.path ||
    capNumber !== stored.cap ||
    marker !== stored.closed_marker ||
    difficulty !== stored.difficulty_label ||
    lane !== stored.lane_label ||
    (pattern.trim() !== "" && pattern !== stored.pattern);
  const save = useMutation({
    mutationFn: () =>
      editBacklog(project.id, {
        repository,
        path: path.trim(),
        cap: capNumber,
        closed_marker: marker,
        difficulty_label: difficulty,
        lane_label: lane,
        ...(pattern.trim() === "" ? {} : { pattern }),
      }),
    onSuccess: (next) => {
      queryClient.setQueryData(keys.backlog(project.id), next);
    },
  });
  const sync = useMutation({
    mutationFn: (dryRun: boolean) => syncBacklog(project.id, dryRun),
    onSuccess: (done) => {
      setReport(done);
      void queryClient.invalidateQueries({ queryKey: keys.snapshot });
    },
  });
  const failure = save.error ?? sync.error;
  return (
    <form
      className="flex flex-col gap-5"
      onSubmit={(e) => {
        e.preventDefault();
        if (canEdit && changed && valid && !save.isPending) save.mutate();
      }}
    >
      <Field
        label="Repository"
        hint="The project's repository that holds the file and gets the issues."
      >
        <Select
          aria-label="Backlog repository"
          value={repository}
          disabled={!canEdit}
          onChange={setRepository}
          options={names.length === 0 ? [["", "No repository"]] : names}
        />
      </Field>
      <Field
        label="File"
        hint="Its path in the default branch, such as notes/backlog/optimisation-backlog.md."
      >
        <Input
          value={path}
          disabled={!canEdit}
          autoComplete="off"
          placeholder="notes/backlog/backlog.md"
          onChange={(e) => setPath(e.target.value)}
        />
      </Field>
      <Field
        label="Most issues one sync makes"
        hint="Keeps the issues agents may take few; the rest waits for the next sync."
        error={capOk ? undefined : `A number from 1 to ${MOST_CAP}.`}
      >
        <Input
          type="number"
          inputMode="numeric"
          min={1}
          max={MOST_CAP}
          value={cap}
          disabled={!canEdit}
          autoComplete="off"
          onChange={(e) => setCap(e.target.value)}
        />
      </Field>
      <Field
        label="Closed marker"
        hint="Text that marks an item closed, in any case. A file closed/<ID>.md next to the backlog closes one too."
      >
        <Input
          value={marker}
          disabled={!canEdit}
          autoComplete="off"
          onChange={(e) => setMarker(e.target.value)}
        />
      </Field>
      <div className="grid grid-cols-2 gap-4">
        <Field label="Difficulty label" hint="Start of difficulty:easy, medium, hard.">
          <Input
            value={difficulty}
            disabled={!canEdit}
            autoComplete="off"
            onChange={(e) => setDifficulty(e.target.value)}
          />
        </Field>
        <Field label="Lane label" hint="For an item that says a lane is required.">
          <Input
            value={lane}
            disabled={!canEdit}
            autoComplete="off"
            onChange={(e) => setLane(e.target.value)}
          />
        </Field>
      </div>
      <Field
        label="Item pattern"
        hint="A regular expression with the groups id and title; empty keeps the default, a heading that starts with an ID such as K-47."
      >
        <Input
          className="font-mono"
          value={pattern}
          disabled={!canEdit}
          autoComplete="off"
          placeholder={stored?.pattern ?? "default"}
          onChange={(e) => setPattern(e.target.value)}
        />
      </Field>
      {canEdit && (
        <div className="flex flex-wrap gap-2">
          <Button type="submit" size="sm" disabled={!changed || !valid || save.isPending}>
            {save.isPending ? "Saving" : "Save backlog"}
          </Button>
          <Button
            type="button"
            size="sm"
            variant="secondary"
            disabled={stored === null || changed || sync.isPending}
            onClick={() => sync.mutate(true)}
          >
            Preview a sync
          </Button>
          <Button
            type="button"
            size="sm"
            variant="secondary"
            disabled={stored === null || changed || sync.isPending}
            onClick={() => sync.mutate(false)}
          >
            Sync now
          </Button>
        </div>
      )}
      {report && <Report report={report} />}
      {failure && (
        <Notice tone="error">
          <span className="font-semibold">That did not work. </span>
          {isApiError(failure) ? failure.message : "The request could not be made. Try again."}
        </Notice>
      )}
    </form>
  );
}

/** What a sync did, or would do. */
export function Report({ report }: { report: SyncReport }) {
  const lines: [string, string][] = [
    ...report.created.map((s): [string, string] => [
      "Created",
      `${s.number > 0 ? `#${s.number} ` : ""}${s.title}`,
    ]),
    ...report.updated.map((s): [string, string] => ["Updated", `#${s.number} ${s.title}`]),
    ...report.labelled.map((s): [string, string] => ["Labelled", `#${s.number} ${s.title}`]),
    ...report.closed.map((s): [string, string] => [
      "Closed as not planned",
      `#${s.number} ${s.title}`,
    ]),
  ];
  return (
    <div
      data-testid="sync-report"
      className="flex flex-col gap-1 rounded-control bg-sunken p-3 text-base"
    >
      <p className="m-0 font-semibold text-text">
        {report.dry_run ? "A sync would do this" : "Synced"}: {report.items} items in {report.path}
      </p>
      {lines.map(([what, text]) => (
        <p key={`${what}-${text}`} className="m-0 text-secondary">
          <span className="text-muted">{what}</span> {text}
        </p>
      ))}
      {report.deferred.length > 0 && (
        <p className="m-0 text-secondary">
          <span className="text-muted">Waiting for the cap</span> {report.deferred.join(", ")}
        </p>
      )}
      <p className="m-0 text-muted">{report.unchanged} unchanged</p>
      {report.warnings.map((w) => (
        <p key={w} className="m-0 text-error-text">
          {w}
        </p>
      ))}
    </div>
  );
}
