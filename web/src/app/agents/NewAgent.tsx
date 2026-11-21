/**
 * Starting a new agent: first one simple choice in a small window of its own, a blank agent or one that starts from an
 * existing agent (the Coder and the Reviewer, which always exist, and the person's own; not the agents Moebius runs
 * for itself). A copy has all the settings and instructions of the agent it starts from under a name still to be
 * typed, and is saved as any new agent is, with `PUT /v1/agents/{name}`, from the editor window.
 */
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { agent as readAgent } from "../../api/calls/harness";
import { keys } from "../../api/keys";
import type { Agent } from "../../api/types/chat";
import { Button } from "../../ui/components/Button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogTitle,
} from "../../ui/components/Dialog";
import { EditorWindow } from "../../ui/components/EditorWindow";
import { WindowHeader } from "../../ui/components/WindowHeader";
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
  onClose,
}: {
  /** The agents it can start from. */
  agents: Agent[];
  onBlank: () => void;
  onFrom: (name: string) => void;
  onClose: () => void;
}) {
  const [fromAgent, setFromAgent] = useState(false);
  const [name, setName] = useState(agents[0]?.name ?? "");
  const chosen = agents.some((agent) => agent.name === name) ? name : (agents[0]?.name ?? "");
  return (
    <Dialog
      open
      onOpenChange={(next) => {
        if (!next) onClose();
      }}
    >
      <DialogContent
        className="max-w-xl gap-0 p-0"
        data-audit="flush"
        data-testid="editor-window"
        hideCloseButton
      >
        <WindowHeader
          title={<DialogTitle className="truncate text-md font-semibold">New agent</DialogTitle>}
          onClose={onClose}
        />
        <div className="flex min-w-0 flex-col gap-4 p-6">
          <DialogDescription>
            Start from nothing, or from an agent that exists: its model, instructions, skills and
            the rest are copied, and you give the copy a new name.
          </DialogDescription>
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
        </div>
        <DialogFooter className="shrink-0 items-center border-t border-hairline px-6 py-3">
          <Button variant="secondary" size="sm" onClick={onClose}>
            Cancel
          </Button>
          <Button
            size="sm"
            disabled={fromAgent && chosen === ""}
            onClick={() => (fromAgent ? onFrom(chosen) : onBlank())}
          >
            Continue
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

/** The window while an agent is read, or when it cannot be: the same window, with Close and no Save. */
export function AgentWait({
  title,
  onClose,
  children,
}: {
  title: string;
  onClose: () => void;
  children: React.ReactNode;
}) {
  return (
    <EditorWindow open title={title} dirty={false} height="fit" onClose={onClose}>
      {children}
    </EditorWindow>
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
  if (detail.isPending) {
    return (
      <AgentWait title="New agent" onClose={shared.onClose}>
        <SkeletonCard lines={4} message={`Reading ${from}`} />
      </AgentWait>
    );
  }
  if (detail.isError) {
    return (
      <AgentWait title="New agent" onClose={shared.onClose}>
        <Notice tone="error">{`Could not read ${from}. ${detail.error.message}`}</Notice>
      </AgentWait>
    );
  }
  const copy = copyOf(
    detail.data,
    agents.find((agent) => agent.name === from),
  );
  // What a save would replace is nothing: the copy is waiting to be saved from the start.
  return <AgentEditor {...shared} initial={draft ?? copy} stored={blank} />;
}
