/**
 * The Decision model page: what the server has recorded of each kind of decision, the training-set exports a person
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
import { keys } from "../../api/keys";
import type { ShadowedKind } from "../../api/types/improve";
import { Button } from "../../ui/components/Button";
import { Body, Box, CELL, Notice, PageHeader, sayError, Table } from "../kit/Kit";
import {
  agreementLine,
  exportRoute,
  judgements,
  percent,
  recallLine,
  shadowsLine,
  when,
} from "./model";

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
        breadcrumb="Improve"
        title="Decision model"
        context={`${judgements(kindList)} judgements in ${kindList.length} kinds; they leave the server only in an export you start.`}
        actions={
          <Button size="sm" disabled={exporting.isPending} onClick={() => exporting.mutate()}>
            Export training set
          </Button>
        }
      />
      <div className="flex flex-col gap-double">
        {exporting.error && (
          <Box title="The export was refused">
            <Notice tone="error">{sayError(exporting.error, "The export was refused.")}</Notice>
          </Box>
        )}

        <Box title="Kinds of decision">
          {kindList.length === 0 ? (
            <Body>Nothing has been decided yet.</Body>
          ) : (
            <>
              <Table
                head={[
                  "Kind",
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
                      <td className={`${CELL} font-semibold text-high`}>{kind.kind}</td>
                      <td className={CELL}>{kind.count}</td>
                      <td className={CELL}>{kind.with_outcome}</td>
                      <td className={CELL}>{kind.by_person}</td>
                      <td className={CELL}>{kind.held_out}</td>
                      <td className={`${CELL} break-all font-mono text-xs`}>
                        {route ?? <span className="text-low">Not exported yet</span>}
                      </td>
                    </tr>
                  );
                })}
              </Table>
              <Body>The files stay on the server. To write them to a directory, run:</Body>
              <code className="font-mono text-xs text-high">
                moebius export --out {"<directory>"}
              </code>
            </>
          )}
        </Box>

        <Box title="Exports">
          {exportList.length === 0 ? (
            <Body>No export has been made yet.</Body>
          ) : (
            <Table head={["Export", "When", "Judgements", "Held out", "Kinds"]}>
              {exportList.map((each) => (
                <tr key={each.id}>
                  <td className={`${CELL} font-semibold text-high`}>#{each.id}</td>
                  <td className={CELL}>{when(each.at)}</td>
                  <td className={CELL}>{judgements(each.kinds)}</td>
                  <td className={CELL}>{each.kinds.reduce((sum, k) => sum + k.held_out, 0)}</td>
                  <td className={CELL}>{each.kinds.map((k) => k.kind).join(", ")}</td>
                </tr>
              ))}
            </Table>
          )}
        </Box>

        <Box title="Shadowing">
          {!shadows.data?.configured ? (
            <Body>No decision model is configured.</Body>
          ) : (
            <>
              <Body>
                {shadows.data.model ?? "The model"} decides alongside the current decider on
                judgements held out of training. A kind is proposed in the inbox at{" "}
                {shadows.data.min_judgements ?? 0} of them with at least{" "}
                {percent(shadows.data.agreement ?? 1)} agreement.
              </Body>
              {shadows.data.kinds.length === 0 ? (
                <Body>Nothing has been shadowed yet.</Body>
              ) : (
                <Table head={["Kind", "Shadows", "Agreement", "Decided by"]}>
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
                  <div key={takeover.id} className="flex flex-wrap items-center gap-base">
                    <span className="min-w-0 flex-1 text-sm text-high">
                      The decision model decides {takeover.kind}: it agreed on{" "}
                      {percent(takeover.agreement)} of {takeover.count} judgements.
                    </span>
                    <Button
                      size="sm"
                      variant="outline"
                      aria-label={`Release ${takeover.kind}`}
                      disabled={release.isPending}
                      onClick={() => release.mutate(takeover.id)}
                    >
                      Release
                    </Button>
                  </div>
                ))}
            </>
          )}
        </Box>
      </div>
    </>
  );
}

/** One kind of the shadowing table: its shadows, its agreement with a thin bar against the share a takeover needs, and who decides it. */
function ShadowRow({ kind, needed }: { kind: ShadowedKind; needed: number }) {
  const recall = recallLine(kind);
  return (
    <tr>
      <td className={`${CELL} font-semibold text-high`}>{kind.kind}</td>
      <td className={CELL}>{shadowsLine(kind)}</td>
      <td className={CELL}>
        {kind.agreement === null ? (
          <span>{recall ?? "No shadow yet"}</span>
        ) : (
          <div className="flex flex-col gap-half">
            <span className={kind.agreement >= needed ? "text-success" : undefined}>
              {agreementLine(kind.agreement, needed)}
            </span>
            <Bar share={kind.agreement} needed={needed} />
          </div>
        )}
      </td>
      <td className={CELL}>{kind.routed ? "The decision model" : "As before"}</td>
    </tr>
  );
}

/** A thin bar filled up to `share`, with a mark at `needed`. */
function Bar({ share, needed }: { share: number; needed: number }) {
  const at = (value: number) => `${Math.min(100, Math.max(0, value * 100))}%`;
  return (
    <div
      className="relative h-[6px] w-full rounded-full bg-track"
      role="img"
      aria-label={`${percent(share)} of ${percent(needed)} needed`}
    >
      <div
        className={`h-full rounded-full ${share >= needed ? "bg-success" : "bg-amber"}`}
        style={{ width: at(share) }}
      />
      <div className="absolute top-0 h-full w-[2px] bg-high" style={{ left: at(needed) }} />
    </div>
  );
}
