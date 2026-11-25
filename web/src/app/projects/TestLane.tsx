/**
 * The "Test lane" box of a repository on a forge (ADR 0035): which lane builds, tests and measures its changes once the
 * Reviewer approved the code, how that lane isolates what it runs, what the project's `lane.toml` is, and the latest jobs.
 * A lane that is not isolated everywhere shows a warning, since the code of a pull request runs on the machine with what
 * its account can reach. A lane that stopped itself after jobs that failed for its own reasons says so, and a person
 * puts it back to work. The lanes themselves are in `moebius.toml`; only the choice is made here.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { editRepositoryLane, readRepositoryLane, resumeLane } from "../../api/calls/lane";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import type { Lane, LaneJob, RepositoryLane } from "../../api/types/lane";
import { Button } from "../../ui/components/Button";
import { Box } from "../../ui/page/Box";
import { Field, Select } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { SkeletonLines } from "../../ui/page/Skeleton";

const NONE = "";

const ISOLATION: Record<Lane["isolation"], string> = {
  always: "Every command runs in a fresh container with no network",
  external_only: "Pull requests from outside run in a container; your own changes run directly",
  off: "Every job runs directly on the machine",
};

/** What a job's state says, in words. */
export const JOB_STATE: Record<LaneJob["state"], string> = {
  queued: "Waiting",
  running: "Running",
  measured: "Measured",
  passed: "Passed",
  failed: "Failed",
  errored: "The lane failed",
  cancelled: "Cancelled",
};

const short = (commit: string) => commit.slice(0, 7);

export function TestLane({ repository }: { repository: number }) {
  const lane = useQuery({
    queryKey: keys.repositoryLane(repository),
    queryFn: () => readRepositoryLane(repository),
  });
  return (
    <Box title="Test lane" testId="test-lane" strong>
      {lane.data ? (
        <Settings repository={repository} stored={lane.data} />
      ) : lane.isError ? (
        <Notice tone="error">The test lane could not be read. Try again.</Notice>
      ) : (
        <SkeletonLines lines={3} />
      )}
    </Box>
  );
}

function Settings({ repository, stored }: { repository: number; stored: RepositoryLane }) {
  const queryClient = useQueryClient();
  const choose = useMutation({
    mutationFn: (name: string) => editRepositoryLane(repository, name === NONE ? null : name),
    onSuccess: (next) => queryClient.setQueryData(keys.repositoryLane(repository), next),
  });
  const resume = useMutation({
    mutationFn: (name: string) => resumeLane(name),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: keys.lanes }),
  });
  const current = stored.current;
  const failed = choose.error ?? resume.error;
  return (
    <div className="flex max-w-xl flex-col gap-5">
      <p className="m-0 text-base text-secondary">
        A machine of yours that builds, tests and measures a change at its pinned head once the
        Reviewer approved its code, and hands the numbers to the Reviewer. Nothing runs while the
        repository is frozen, agents are paused, the card is held or the change touches a locked
        path.
      </p>
      {stored.lanes.length === 0 ? (
        <p className="m-0 text-base text-secondary" data-testid="no-lanes">
          The server has no lane. Add a [[lanes]] entry to moebius.toml and restart it.
        </p>
      ) : (
        <Field
          label="Lane"
          hint="Which of the server's lanes runs this repository's jobs. None, and nothing is measured here."
        >
          <Select
            value={stored.lane ?? NONE}
            disabled={choose.isPending}
            onChange={(name) => {
              choose.reset();
              choose.mutate(name);
            }}
            options={[
              [NONE, "No lane"],
              ...stored.lanes.map((l): [string, string] => [l.name, l.name]),
            ]}
          />
        </Field>
      )}
      {current?.warning && (
        <Notice tone="error">
          <span data-testid="isolation-warning">
            <span className="font-semibold">{`Isolation is ${current.isolation.replace("_", " ")}. `}</span>
            {current.warning}
          </span>
        </Notice>
      )}
      {current && <Machine lane={current} />}
      {current?.paused && (
        <Notice
          tone="error"
          action={
            <Button
              size="sm"
              disabled={resume.isPending}
              onClick={() => resume.mutate(current.name)}
            >
              Resume the lane
            </Button>
          }
        >
          <span data-testid="lane-paused">
            <span className="font-semibold">The lane paused itself. </span>
            {current.paused.reason}
          </span>
        </Notice>
      )}
      {current && <Definition stored={stored} />}
      {stored.jobs.length > 0 && <Jobs jobs={stored.jobs} />}
      {failed && (
        <Notice tone="error">
          <span className="font-semibold">Couldn't save this. It stays as it was. </span>
          {isApiError(failed) ? failed.message : "The lane could not be changed."}
        </Notice>
      )}
    </div>
  );
}

/** What the report will say of the machine, and how it isolates. */
function Machine({ lane }: { lane: Lane }) {
  const facts: Array<[string, string | null]> = [
    ["Machine", lane.runner ?? "the server itself"],
    ["GPUs", lane.gpus],
    ["Power cap", lane.power_cap],
    ["ROCm", lane.rocm],
    ["Driver", lane.driver],
    ["Container", lane.isolation === "off" ? null : lane.image],
    ["Isolation", ISOLATION[lane.isolation]],
  ];
  return (
    <dl
      className="m-0 grid grid-cols-[max-content_1fr] gap-x-4 gap-y-1 text-base"
      data-testid="lane-machine"
    >
      {facts
        .filter(([, value]) => value)
        .map(([label, value]) => (
          <div key={label} className="contents">
            <dt className="text-sm font-semibold text-secondary">{label}</dt>
            <dd className="m-0 min-w-0 break-words text-text">{value}</dd>
          </div>
        ))}
      <div className="contents">
        <dt className="text-sm font-semibold text-secondary">Queue</dt>
        <dd className="m-0 text-text">
          {lane.running ? `1 running, ${lane.queued} waiting` : `${lane.queued} waiting`}
        </dd>
      </div>
    </dl>
  );
}

function Definition({ stored }: { stored: RepositoryLane }) {
  const { definition } = stored;
  if (definition.problem) {
    return (
      <Notice tone="error">
        <span data-testid="lane-definition">{`The project's lane.toml is not valid: ${definition.problem}`}</span>
      </Notice>
    );
  }
  return (
    <p className="m-0 text-base text-secondary" data-testid="lane-definition">
      {definition.present
        ? `The project's lane.toml has ${definition.steps.length} ${definition.steps.length === 1 ? "step" : "steps"}: ${definition.steps.join(", ")}${definition.measures ? ", and measures." : ", and no measurement."}`
        : "The project has no lane.toml in the harness configuration repository, so no job is queued."}
    </p>
  );
}

function Jobs({ jobs }: { jobs: LaneJob[] }) {
  return (
    <div className="flex flex-col gap-1" data-testid="lane-jobs">
      <span className="text-sm font-semibold text-secondary">Latest jobs</span>
      <ul className="m-0 flex list-none flex-col gap-1 p-0">
        {jobs.map((job) => (
          <li key={job.id} className="flex flex-wrap items-baseline gap-x-3 text-base text-text">
            <span className="font-mono text-sm text-secondary">{short(job.head)}</span>
            <span>{JOB_STATE[job.state]}</span>
            {job.state === "running" && job.step && (
              <span className="text-sm text-muted">{job.step}</span>
            )}
            {job.summary && job.state !== "running" && (
              <span className="min-w-0 text-sm text-muted [overflow-wrap:anywhere]">
                {job.summary}
              </span>
            )}
          </li>
        ))}
      </ul>
    </div>
  );
}
