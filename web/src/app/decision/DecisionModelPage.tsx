/**
 * The Decisions page, as the sidebar calls it — the decision model is the thing it shows: what the server has
 * recorded of each kind of decision, the training-set exports a person
 * started, and the shadowing of a trained model: how often it chooses what was recorded, per kind, against the share a
 * takeover needs, and the kinds it has taken over, which a person can release. "Export training set" is
 * `POST /v1/exports`; the files stay on the server, so each kind shows the route that downloads its file and the
 * command that writes them elsewhere. A takeover proposal itself is an item of the inbox.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  judgementKinds,
  releaseTakeover,
  shadowing,
  startExport,
  trainingExports,
} from "../../api/calls/improve";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import type { ShadowedKind } from "../../api/types/improve";
import { Button } from "../../ui/components/Button";
import { plural } from "../../ui/lib/format";
import { Box } from "../../ui/page/Box";
import { CELL, Table } from "../../ui/page/List";
import { Notice } from "../../ui/page/Notice";
import { Body, PageHeader } from "../../ui/page/PageHeader";
import { Status } from "../../ui/page/Status";
import { DeleteExport } from "./DeleteExport";

import {
  agreementLine,
  exportRoute,
  judgements,
  percent,
  recallLine,
  shadowsLine,
  when,
} from "./model";

/** A cell that holds a number, so the digits of a column line up as they change. */
const NUMBER = `${CELL} tabular-nums`;

export function DecisionModelPage() {
  const queryClient = useQueryClient();
  const kinds = useQuery({ queryKey: keys.kinds, queryFn: judgementKinds });
  const exports = useQuery({ queryKey: keys.exports, queryFn: trainingExports });
  const shadows = useQuery({ queryKey: keys.shadowing, queryFn: shadowing });
  const refresh = () => queryClient.invalidateQueries({ queryKey: ["decision-model"] });
  const exporting = useMutation({ mutationFn: startExport, onSuccess: refresh });
  const release = useMutation({ mutationFn: releaseTakeover, onSuccess: refresh });

  const kindList = kinds.data ?? [];
  const exportList = exports.data ?? [];
  return (
    <>
      <PageHeader
        title="Decisions"
        context={
          kinds.isPending
            ? "Reading what the server has recorded."
            : kinds.isError
              ? // The counts of a page that could not be read would say the opposite of what happened.
                "Nothing was read, so these counts are not known."
              : `${plural(judgements(kindList), "judgement", "judgements")} in ${plural(
                  kindList.length,
                  "type",
                  "types",
                )}`
        }
        actions={
          <Button size="sm" disabled={exporting.isPending} onClick={() => exporting.mutate()}>
            {exporting.isPending ? "Exporting" : "Export training set"}
          </Button>
        }
      />
      <div className="flex flex-col gap-8">
        {exporting.error && (
          <Box title="The export was refused">
            <Notice tone="error">{sayError(exporting.error, "The export was refused.")}</Notice>
          </Box>
        )}

        <Box title="Decision types">
          {kinds.isPending ? (
            <Body>Reading the decision types.</Body>
          ) : kinds.isError ? (
            <Notice
              tone="error"
              action={
                <Button
                  size="sm"
                  variant="secondary"
                  disabled={kinds.isFetching}
                  onClick={() => void kinds.refetch()}
                >
                  Try again
                </Button>
              }
            >
              {`Could not read the decision types. ${kinds.error.message}`}
            </Notice>
          ) : kindList.length === 0 ? (
            <Body>Nothing has been decided yet.</Body>
          ) : (
            <div className="flex flex-col gap-4">
              <Table
                head={[
                  "Type",
                  "Judgements",
                  "With an outcome",
                  "By a person",
                  "Held out",
                  "Its file",
                ]}
              >
                {kindList.map((kind) => {
                  const route = exportRoute(exportList, kind.kind);
                  return (
                    <tr key={kind.kind}>
                      <td className={`${CELL} font-semibold text-text`}>{kind.kind}</td>
                      <td className={NUMBER}>{kind.count}</td>
                      <td className={NUMBER}>{kind.with_outcome}</td>
                      <td className={NUMBER}>{kind.by_person}</td>
                      <td className={NUMBER}>{kind.held_out}</td>
                      <td className={`${CELL} break-all font-mono text-sm`}>
                        {route ?? <span className="text-secondary">Not exported yet</span>}
                      </td>
                    </tr>
                  );
                })}
              </Table>
              <Body>The files stay on the server. To write them to a directory, run:</Body>
              <code className="block w-fit rounded-control bg-code px-3 py-2 font-mono text-sm text-text">
                moebius export --out {"<directory>"}
              </code>
            </div>
          )}
        </Box>

        <Box title="Exports">
          {exports.isPending ? (
            <Body>Reading the exports.</Body>
          ) : exports.isError ? (
            <Notice
              tone="error"
              action={
                <Button
                  size="sm"
                  variant="secondary"
                  disabled={exports.isFetching}
                  onClick={() => void exports.refetch()}
                >
                  Try again
                </Button>
              }
            >
              {`Could not read the exports. ${exports.error.message}`}
            </Notice>
          ) : exportList.length === 0 ? (
            <Body>No export has been made yet.</Body>
          ) : (
            <Table head={["Export", "When", "Judgements", "Held out", "Types", "Delete"]}>
              {exportList.map((each) => (
                <tr key={each.id}>
                  <td className={`${CELL} font-mono text-sm font-semibold text-text tabular-nums`}>
                    #{each.id}
                  </td>
                  <td className={`${CELL} tabular-nums`}>{when(each.at)}</td>
                  <td className={NUMBER}>{judgements(each.kinds)}</td>
                  <td className={NUMBER}>{each.kinds.reduce((sum, k) => sum + k.held_out, 0)}</td>
                  <td className={CELL}>{each.kinds.map((k) => k.kind).join(", ")}</td>
                  <td className={CELL}>
                    <DeleteExport id={each.id} />
                  </td>
                </tr>
              ))}
            </Table>
          )}
        </Box>

        <Box title="Shadowing">
          {shadows.isPending ? (
            <Body>Reading the shadowing.</Body>
          ) : shadows.isError ? (
            <Notice
              tone="error"
              action={
                <Button
                  size="sm"
                  variant="secondary"
                  disabled={shadows.isFetching}
                  onClick={() => void shadows.refetch()}
                >
                  Try again
                </Button>
              }
            >
              {`Could not read the shadowing. ${shadows.error.message}`}
            </Notice>
          ) : !shadows.data?.configured ? (
            <Body>No decision model is configured.</Body>
          ) : (
            <div className="flex flex-col gap-6">
              <Body>
                {shadows.data.model ?? "The model"} decides alongside the current decider on
                judgements held out of training. A type is proposed in the inbox at{" "}
                <span className="tabular-nums">{shadows.data.min_judgements ?? 0}</span> of them
                with at least{" "}
                <span className="tabular-nums">{percent(shadows.data.agreement ?? 1)}</span>{" "}
                agreement.
              </Body>
              {shadows.data.kinds.length === 0 ? (
                <Body>Nothing has been shadowed yet.</Body>
              ) : (
                <Table head={["Type", "Shadows", "Agreement", "Decided by"]}>
                  {shadows.data.kinds.map((kind) => (
                    <ShadowRow key={kind.kind} kind={kind} needed={shadows.data.agreement ?? 1} />
                  ))}
                </Table>
              )}
              {release.error && (
                <Notice tone="error">{sayError(release.error, "Could not release it.")}</Notice>
              )}
              {shadows.data.takeovers
                .filter((takeover) => takeover.status === "merged")
                .map((takeover) => (
                  <div key={takeover.id} className="flex flex-wrap items-center gap-4">
                    <span className="min-w-0 flex-1 text-base text-text tabular-nums">
                      The decision model decides {takeover.kind}: it agreed on{" "}
                      {percent(takeover.agreement)} of{" "}
                      {plural(takeover.count, "judgement", "judgements")}.
                    </span>
                    <Button
                      size="sm"
                      variant="secondary"
                      aria-label={`Release ${takeover.kind}`}
                      disabled={release.isPending}
                      onClick={() => release.mutate(takeover.id)}
                    >
                      Release
                    </Button>
                  </div>
                ))}
            </div>
          )}
        </Box>
      </div>
    </>
  );
}

/** One kind of the shadowing table: its shadows, its agreement against the share a takeover needs, and who decides it. */
function ShadowRow({ kind, needed }: { kind: ShadowedKind; needed: number }) {
  const recall = recallLine(kind);
  const ready = kind.agreement !== null && kind.agreement >= needed;
  return (
    <tr>
      <td className={`${CELL} font-semibold text-text`}>{kind.kind}</td>
      <td className={`${CELL} tabular-nums`}>{shadowsLine(kind)}</td>
      <td className={CELL}>
        {kind.agreement === null ? (
          <span className="text-secondary">{recall ?? "No shadow yet"}</span>
        ) : (
          <div className="flex flex-col gap-2">
            <span className="flex flex-wrap items-center gap-3">
              <span className="tabular-nums text-text">
                {agreementLine(kind.agreement, needed)}
              </span>
              <Status tone={ready ? "complete" : "attention"}>
                {ready ? "Enough" : "Not enough"}
              </Status>
            </span>
            <Bar share={kind.agreement} needed={needed} />
          </div>
        )}
      </td>
      <td className={CELL}>{kind.routed ? "The decision model" : "As before"}</td>
    </tr>
  );
}

/** A thin bar filled up to `share`, with a mark at `needed`; the words beside it carry the state, not the colour. */
function Bar({ share, needed }: { share: number; needed: number }) {
  const at = (value: number) => `${Math.min(100, Math.max(0, value * 100))}%`;
  return (
    <div
      className="relative h-2 w-full max-w-60 rounded-full bg-track"
      role="img"
      aria-label={`${percent(share)} of ${percent(needed)} needed`}
    >
      <div
        className={`h-full rounded-full ${share >= needed ? "bg-complete" : "bg-attention"}`}
        style={{ width: at(share) }}
      />
      <div className="absolute top-0 h-full w-0.5 bg-text" style={{ left: at(needed) }} />
    </div>
  );
}
