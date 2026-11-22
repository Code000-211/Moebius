/**
 * The Instructions tab of an agent's page: the agent's instructions in one large editor that fills the tab (the code
 * editor's own colours, `--editor-bg` and `--editor-text`, light in the light theme; mono 13px, long lines wrap), or
 * asked of the Moebius agent, which writes and saves them (a chat started with the request, linked once it has it). What
 * a built-in agent brings itself is shown above the editor, read only, in the same colours.
 */
import { useMutation } from "@tanstack/react-query";
import { useState } from "react";
import { Link } from "react-router";
import { startChat } from "../../api/calls/chat";
import type { BuiltInAgent } from "../../api/types/harness";
import { Button } from "../../ui/components/Button";
import { Chip } from "../../ui/page/Chip";
import { TextArea } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { instructionsRequest } from "../harness/importRequest";
import type { AgentForm } from "./model";

export function AgentInstructions({
  form,
  builtIn,
  onChange,
  onProblem,
}: {
  form: AgentForm;
  builtIn: BuiltInAgent | null;
  onChange: (patch: Partial<AgentForm>) => void;
  onProblem: (problem: string) => void;
}) {
  const [byAgent, setByAgent] = useState(false);
  const [wish, setWish] = useState("");
  const ask = useMutation({
    mutationFn: () =>
      startChat({ agent: "Moebius", message: instructionsRequest(form.name, wish.trim()) }),
    onError: (error) => onProblem(error.message),
  });
  const asking = byAgent && !form.naming;
  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3">
      {!form.naming && (
        <div className="flex shrink-0 flex-wrap gap-3">
          <Chip selected={!byAgent} onClick={() => setByAgent(false)}>
            Write manually
          </Chip>
          <Chip selected={byAgent} onClick={() => setByAgent(true)}>
            Write with agent
          </Chip>
        </div>
      )}
      {builtIn && (
        <section aria-label="Built-in instructions" className="flex shrink-0 flex-col gap-1">
          <span className="flex items-baseline justify-between text-sm text-muted">
            <span className="font-medium text-secondary">Built-in instructions</span>
            Read only
          </span>
          <pre
            data-testid="built-in-instructions"
            className="m-0 max-h-40 overflow-auto whitespace-pre-wrap rounded-control border border-hairline-strong bg-editor-bg p-3 font-mono text-[13px] leading-relaxed text-editor-text"
          >
            {builtIn.instructions}
          </pre>
        </section>
      )}
      {asking ? (
        <div className="flex flex-col gap-4">
          <TextArea
            mono={false}
            rows={5}
            aria-label="What this agent should do"
            value={wish}
            placeholder="What this agent should do and how. The Moebius agent writes and saves the instructions."
            onChange={(event) => setWish(event.target.value)}
          />
          {ask.isSuccess ? (
            <div className="flex flex-wrap items-center gap-3">
              <Notice tone="ok">The Moebius agent has the request.</Notice>
              <Button asChild variant="secondary" size="sm">
                <Link to={`/chat/${ask.data.id}`}>Open the chat</Link>
              </Button>
            </div>
          ) : (
            <div className="flex flex-wrap items-center gap-3">
              {wish.trim() === "" && !ask.isPending && (
                <span className="text-sm text-secondary">
                  Say what the agent should do, then ask
                </span>
              )}
              <Button
                size="sm"
                disabled={wish.trim() === "" || ask.isPending}
                onClick={() => ask.mutate()}
              >
                {ask.isPending ? "Sending" : "Submit"}
              </Button>
            </div>
          )}
        </div>
      ) : (
        <textarea
          aria-label="Instructions"
          data-testid="instructions-editor"
          spellCheck={false}
          wrap="soft"
          value={form.instructions}
          placeholder="What the agent is told before it starts, in Markdown."
          onChange={(event) => onChange({ instructions: event.target.value })}
          className="min-h-64 w-full min-w-0 flex-1 resize-none whitespace-pre-wrap rounded-control border border-hairline-strong bg-editor-bg p-4 font-mono text-[13px] leading-relaxed text-editor-text outline-none placeholder:text-muted focus-visible:border-accent"
        />
      )}
    </div>
  );
}
