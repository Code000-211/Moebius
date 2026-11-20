/**
 * The instructions box of the agent editor: the instructions written by hand, or asked of the Moebius agent, which
 * writes and saves them (a chat started with the request, linked once it has it).
 */
import { useMutation } from "@tanstack/react-query";
import { useState } from "react";
import { Link } from "react-router";
import { startChat } from "../../api/calls/chat";
import { Button } from "../../ui/components/Button";
import { Box } from "../../ui/page/Box";
import { Chip } from "../../ui/page/Chip";
import { TextArea } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { instructionsRequest } from "../harness/importRequest";
import type { AgentForm } from "./model";

export function AgentInstructions({
  form,
  onChange,
  onProblem,
}: {
  form: AgentForm;
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
  return (
    <Box title="Instructions" className="flex flex-col gap-4 [&>header]:mb-0">
      {!form.naming && (
        <div className="flex flex-wrap gap-3">
          <Chip selected={!byAgent} onClick={() => setByAgent(false)}>
            Write manually
          </Chip>
          <Chip selected={byAgent} onClick={() => setByAgent(true)}>
            Write with agent
          </Chip>
        </div>
      )}
      {byAgent && !form.naming ? (
        <>
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
        </>
      ) : (
        <TextArea
          rows={14}
          aria-label="Instructions"
          value={form.instructions}
          placeholder="What the agent is told before it starts, in Markdown."
          onChange={(event) => onChange({ instructions: event.target.value })}
        />
      )}
    </Box>
  );
}
