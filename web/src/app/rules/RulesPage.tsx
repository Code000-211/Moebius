/**
 * The Rules page: the built-in patterns and what each refused this week, the server's rules in prose and the rules
 * each repository adds. The rules are cards that grow into editors (`RuleList`); "Write with agent" asks the
 * Moebius agent in a chat instead. The server's rules are saved with `PUT /v1/rules`, a repository's with
 * `PUT /v1/repositories/{id}/files/.moebius/rules.md`; the server checks both and says which line is wrong.
 *
 * The page reads from what cannot be changed to what a person can: the patterns first, then the server's own rules,
 * then one box per repository.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Link } from "react-router";
import { startChat } from "../../api/calls/chat";
import { rules, saveRepositoryRules, saveRules } from "../../api/calls/rules";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import { Tag, type TagTone } from "../../ui/chat/Tag";
import { Button } from "../../ui/components/Button";
import { Box } from "../../ui/page/Box";
import { Field, TextArea } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { Body, PageHeader } from "../../ui/page/PageHeader";
import { Reading, SkeletonCard } from "../../ui/page/Skeleton";
import { agentRequest, refusalsLabel } from "./file";
import { RuleList } from "./RuleList";

export function RulesPage() {
  const queryClient = useQueryClient();
  const data = useQuery({ queryKey: keys.rules, queryFn: rules });
  const refresh = () => queryClient.invalidateQueries({ queryKey: keys.rules });
  const [byAgent, setByAgent] = useState(false);
  const [wish, setWish] = useState("");
  const ask = useMutation({
    mutationFn: () => startChat({ agent: "Moebius", message: agentRequest(wish.trim()) }),
    onSuccess: () => {
      setByAgent(false);
      setWish("");
    },
  });

  const header = (
    <PageHeader
      title="Rules"
      context="What a run must not do. It lives in rules.md, and a refusal names the rule it broke."
    />
  );
  if (data.isPending) {
    return (
      <>
        {header}
        <Reading message="Reading the rules" className="flex flex-col gap-6">
          <SkeletonCard lines={2} />
          <SkeletonCard lines={5} />
        </Reading>
      </>
    );
  }
  if (data.error || !data.data) {
    return (
      <>
        {header}
        <Notice tone="error">{sayError(data.error, "Could not read the rules. Try again.")}</Notice>
      </>
    );
  }
  const view = data.data;
  const prose = view.prose;

  return (
    <>
      {header}
      <div className="flex flex-col gap-6">
        <Box
          title="Built-in patterns"
          aside="Always on. Checked before any model is asked."
          className="min-w-0"
        >
          <div className="flex flex-col gap-4">
            {view.patterns.map((pattern) => (
              <PatternRow
                key={pattern.name}
                name={pattern.name}
                description={pattern.description}
                refusals={refusalsLabel(view.refusals, pattern.name)}
                refused={(view.refusals[pattern.name] ?? 0) > 0}
              />
            ))}
          </div>
        </Box>

        <Box
          title="Rules in prose"
          actions={
            prose.editable && !byAgent ? (
              <Button size="sm" variant="secondary" onClick={() => setByAgent(true)}>
                Write with agent
              </Button>
            ) : undefined
          }
          className="min-w-0"
        >
          <div className="flex flex-col gap-4">
            {prose.editable && byAgent && (
              <div className="flex flex-col gap-4 rounded-card bg-sunken p-4">
                <Field
                  label="What should be allowed or forbidden"
                  hint="The Moebius agent reads the rules, writes the change and saves them."
                >
                  <TextArea
                    rows={4}
                    value={wish}
                    onChange={(event) => setWish(event.target.value)}
                  />
                </Field>
                <div className="flex flex-wrap gap-2">
                  <Button
                    size="sm"
                    disabled={ask.isPending || wish.trim() === ""}
                    onClick={() => ask.mutate()}
                  >
                    {ask.isPending ? "Sending" : "Submit"}
                  </Button>
                  <Button size="sm" variant="secondary" onClick={() => setByAgent(false)}>
                    Cancel
                  </Button>
                </div>
                {ask.error && (
                  <Notice tone="error">
                    {sayError(ask.error, "Could not ask the agent. Try again.")}
                  </Notice>
                )}
              </div>
            )}
            {ask.isSuccess && (
              <div className="flex flex-col gap-4 sm:flex-row sm:items-center">
                <div className="min-w-0 flex-1">
                  <Notice>The Moebius agent is writing the rules.</Notice>
                </div>
                <Button asChild size="sm" variant="secondary">
                  <Link to={`/chat/${ask.data.id}`}>Open the chat</Link>
                </Button>
              </div>
            )}
            {prose.error && <Notice tone="error">{prose.error}</Notice>}
            <RuleList
              rules={prose.rules}
              file={prose.text}
              refusals={view.refusals}
              editable={prose.editable}
              empty="No rule in prose. Only the built-in patterns apply."
              onSave={async (file) => {
                const saved = await saveRules(file);
                await refresh();
                return saved.commit;
              }}
            />
            {!prose.editable && <Body>Read only: this server has no rules file.</Body>}
          </div>
        </Box>

        {view.repositories.map((repository) => (
          <Box
            key={repository.repository_id}
            title={repository.repository}
            aside="Added to the server's rules; cannot weaken them"
            className="min-w-0"
          >
            <div className="flex flex-col gap-4">
              {repository.error && <Notice tone="error">{repository.error}</Notice>}
              <RuleList
                rules={repository.rules}
                file={repository.text}
                refusals={view.refusals}
                editable
                empty="No rule of its own."
                onSave={async (file) => {
                  await saveRepositoryRules(repository.repository_id, file);
                  await refresh();
                  return undefined;
                }}
              />
              <Body>Saved as a commit on the default branch (.moebius/rules.md).</Body>
            </div>
          </Box>
        ))}
      </div>
    </>
  );
}

/** A built-in pattern: what it is called, what it stops, and how often it did. */
function PatternRow({
  name,
  description,
  refusals,
  refused,
}: {
  name: string;
  description: string;
  refusals: string;
  refused: boolean;
}) {
  const tone: TagTone = refused ? "attention" : "plain";
  return (
    <div className="flex flex-col gap-4 rounded-card bg-sunken p-4 xl:flex-row xl:items-baseline xl:gap-6">
      <span className="min-w-0 break-words font-mono text-sm font-semibold text-text xl:w-56 xl:shrink-0">
        {name}
      </span>
      <span className="min-w-0 flex-1 break-words text-base text-text">{description}</span>
      <Tag tone={tone} className="tabular-nums xl:shrink-0">
        {refusals}
      </Tag>
    </div>
  );
}
