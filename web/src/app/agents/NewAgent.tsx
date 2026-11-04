/**
 * Starting a new agent: first one simple choice, a blank agent or one that starts from an existing agent (the Coder
 * and the Reviewer, which always exist, and the person's own; not the agents Moebius runs for itself). A copy has all
 * the settings and instructions of the agent it starts from under a name still to be typed, and is saved as any new
 * agent is, with `PUT /v1/agents/{name}`.
 */
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { agent as readAgent } from "../../api/calls/harness";
import { keys } from "../../api/keys";
import type { Agent } from "../../api/types/chat";
import { Button } from "../../ui/components/Button";
import { Box } from "../../ui/page/Box";
import { Chip } from "../../ui/page/Chip";
import { Field, Select } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { SkeletonCard } from "../../ui/page/Skeleton";
import { AgentEditor, type EditorProps } from "./AgentEditor";
import { type AgentForm, blankForm, copyOf } from "./model";

/** The choice a new agent starts with: blank, or from the agent chosen. */
export function NewAgentChoice({
  agents,
  onBlank,
  onFrom,
}: {
  /** The agents it can start from. */
  agents: Agent[];
  onBlank: () => void;
  onFrom: (name: string) => void;
}) {
  const [fromAgent, setFromAgent] = useState(false);
  const [name, setName] = useState(agents[0]?.name ?? "");
  const chosen = agents.some((agent) => agent.name === name) ? name : (agents[0]?.name ?? "");
  return (
    <Box title="New agent" className="flex flex-col gap-4">
      <p className="m-0 max-w-prose text-base text-secondary">
        Start from nothing, or from an agent that exists: its model, instructions, skills and the
        rest are copied, and you give the copy a new name.
      </p>
      <div className="flex flex-wrap gap-3">
        <Chip selected={!fromAgent} onClick={() => setFromAgent(false)}>
          Blank
        </Chip>
        <Chip selected={fromAgent} onClick={() => setFromAgent(true)}>
          From an existing agent
        </Chip>
      </div>
      {fromAgent && (
        <Field label="Start from">
          {(id) => (
            <Select
              id={id}
              options={agents.map((agent) => agent.name)}
              value={chosen}
              onChange={setName}
            />
          )}
        </Field>
      )}
      <div>
        <Button
          size="sm"
          disabled={fromAgent && chosen === ""}
          onClick={() => (fromAgent ? onFrom(chosen) : onBlank())}
        >
          Continue
        </Button>
      </div>
    </Box>
  );
}

/** The editor of a new agent: blank, or a copy of the agent `from`, read from the server. */
export function NewAgentEditor({
  from,
  agents,
  draft,
  ...shared
}: Omit<EditorProps, "initial" | "stored" | "broken"> & {
  from: string | null;
  agents: Agent[];
  draft: AgentForm | null;
}) {
  const blank = blankForm("", true);
  const detail = useQuery({
    queryKey: keys.agent(from ?? ""),
    queryFn: () => readAgent(from as string),
    enabled: from !== null,
    retry: false,
  });
  if (from === null) return <AgentEditor {...shared} initial={draft ?? blank} stored={blank} />;
  if (detail.isPending) return <SkeletonCard lines={4} message={`Reading ${from}`} />;
  if (detail.isError) {
    return <Notice tone="error">{`Could not read ${from}. ${detail.error.message}`}</Notice>;
  }
  const copy = copyOf(
    detail.data,
    agents.find((agent) => agent.name === from),
  );
  // What a save would replace is nothing: the copy is waiting to be saved from the start.
  return <AgentEditor {...shared} initial={draft ?? copy} stored={blank} />;
}
