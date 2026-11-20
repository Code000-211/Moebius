/**
 * The brief of the research chosen on the Experiments page: what it is after, how far it has got, and what a person can
 * do with it now (confirm, start, stop, delete, and for a draft change its direction or open the editor).
 */
import { PencilSimpleIcon } from "@phosphor-icons/react";
import { useQueryClient } from "@tanstack/react-query";
import { editResearch } from "../../api/calls/improve";
import { keys } from "../../api/keys";
import type { Research } from "../../api/types/improve";
import { Tag } from "../../ui/chat/Tag";
import { Button } from "../../ui/components/Button";
import { IconButton } from "../../ui/components/IconButton";
import { Box } from "../../ui/page/Box";
import { EditableProperty } from "../../ui/page/EditableProperty";
import { Body } from "../../ui/page/PageHeader";
import { PageProperties, Property } from "../../ui/page/Properties";
import { DeleteBrief } from "./DeleteBrief";
import { actions, limitReached, statusTag, usage } from "./model";

/** The brief of the research chosen: what it is after, how far it has got, and what a person can do with it now. */
export function BriefBox({
  research,
  repository,
  busy,
  onEdit,
  onDeleted,
  onAct,
}: {
  research: Research;
  repository: string;
  busy: boolean;
  onEdit: () => void;
  onDeleted: () => void;
  onAct: (action: "confirm" | "start" | "stop") => void;
}) {
  const status = statusTag(research.status);
  const direction = research.better === "higher" ? "Higher is better" : "Lower is better";
  const possible = actions(research.status);
  const queryClient = useQueryClient();
  // A draft's brief is changed one property at a time, saved alone; the research is read again once it is answered.
  const saveBrief = async (change: Partial<Pick<Research, "better">>) => {
    const { goal, measure, better, budget } = { ...research, ...change };
    try {
      await editResearch(research.id, { goal, measure, better, budget });
    } finally {
      void queryClient.invalidateQueries({ queryKey: keys.research });
    }
  };
  return (
    <Box
      title="Brief"
      actions={
        research.status === "draft" ? (
          <IconButton
            icon={PencilSimpleIcon}
            aria-label="Edit the brief"
            title="Edit the brief"
            onClick={onEdit}
          />
        ) : undefined
      }
    >
      <div className="flex flex-col gap-4">
        <h3 className="m-0 min-w-0 break-words text-lg font-semibold text-text">{research.goal}</h3>
        <div className="flex flex-wrap items-center gap-2">
          <Tag tone={status.tone}>{status.label}</Tag>
          <span className="text-sm text-secondary tabular-nums">
            {repository} · {direction} · {usage(research)}
          </span>
        </div>
        <PageProperties label={`Properties of ${research.goal}`}>
          <Property name="Status">
            <Tag tone={status.tone}>{status.label}</Tag>
          </Property>
          <Property name="Repository">{repository}</Property>
          {research.status === "draft" ? (
            <EditableProperty
              name="Direction"
              editor={{
                kind: "choice",
                options: [
                  ["higher", "Higher is better"],
                  ["lower", "Lower is better"],
                ],
                value: research.better,
                onSave: (better) => saveBrief({ better: better as Research["better"] }),
              }}
            >
              {direction}
            </EditableProperty>
          ) : (
            <Property name="Direction">{direction}</Property>
          )}
          <Property name="Budget">{usage(research)}</Property>
        </PageProperties>
        <span className="min-w-0 break-words font-mono text-sm text-secondary">
          {research.measure}
        </span>
        {limitReached(research) && <Body>{limitReached(research)}</Body>}
        {research.reason && <Body>{research.reason}</Body>}
        <div className="flex flex-wrap gap-2">
          {possible.map(({ label, action, primary }) => (
            <Button
              key={label}
              size="sm"
              variant={primary ? "primary" : "secondary"}
              disabled={busy}
              onClick={() => onAct(action)}
            >
              {label}
            </Button>
          ))}
          <DeleteBrief research={research} onDeleted={onDeleted} />
        </div>
      </div>
    </Box>
  );
}
