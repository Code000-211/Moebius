/** The form of an `agent` node: the agent that runs, the prompt it is given and the output port it declares. */

import { useQuery } from "@tanstack/react-query";
import { agents } from "../../../api/calls/chat";
import { keys } from "../../../api/keys";
import { Input } from "../../../ui/components/Input";
import { Field, Select, TextArea } from "../../../ui/page/Field";
import type { NodeFormProps } from "../nodeKinds";
import { ParamsPreview } from "./preview";

export function AgentForm({ node, onParams }: NodeFormProps) {
  const params = (node.params ?? {}) as Record<string, unknown>;
  const agent = typeof params.agent === "string" ? params.agent : "";
  const prompt = typeof params.prompt === "string" ? params.prompt : "";
  const output = typeof params.output === "string" ? params.output : "";
  const listed = useQuery({ queryKey: keys.agents, queryFn: agents });
  const names = (listed.data ?? []).map((each): [string, string] => [each.name, each.name]);
  if (agent !== "" && !names.some(([name]) => name === agent)) names.push([agent, agent]);
  const missingPrompt = prompt.trim() === "";
  const missingOutput = output.trim() === "";

  function give(name: string, value: unknown) {
    const next = { ...params };
    if (value === undefined || value === "") {
      delete next[name];
    } else {
      next[name] = value;
    }
    onParams(next);
  }

  return (
    <div className="flex min-w-0 flex-col gap-4" data-testid="agent-form">
      <Field
        label="Agent"
        hint="An agent of the harness configuration repository, which runs in a worktree the run owns."
        error={agent === "" ? "An agent node needs the agent that runs." : undefined}
      >
        <Select
          aria-label="Agent"
          value={agent}
          onChange={(value) => give("agent", value)}
          options={[["", "Choose an agent"], ...names]}
        />
      </Field>
      <Field
        label="Prompt"
        hint="What the agent reads. Expressions such as {{ trigger.tag }} are filled before it starts."
        error={missingPrompt ? "An agent node needs its prompt." : undefined}
      >
        <TextArea
          aria-label="Prompt"
          mono
          rows={8}
          value={prompt}
          onChange={(event) => give("prompt", event.target.value)}
          placeholder="Read {{ trigger.issue }} and answer."
        />
      </Field>
      <Field
        label="Output"
        hint="The name of the port the node's value leaves by, the fenced JSON block the agent ends with."
        error={missingOutput ? "An agent node declares the name of its output port." : undefined}
      >
        <Input
          aria-label="Output"
          mono
          value={output}
          placeholder="summary"
          onChange={(event) => give("output", event.target.value)}
        />
      </Field>
      <ParamsPreview params={params} />
    </div>
  );
}
