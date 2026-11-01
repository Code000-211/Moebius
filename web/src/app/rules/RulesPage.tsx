/**
 * The Rules page: the built-in patterns and what each refused this week, the server's rules in prose and the rules
 * each repository adds. The rules are cards that grow into editors (`RuleList`); "Let the Moebius agent write them"
 * asks the Moebius agent in a chat instead. The server's rules are saved with `PUT /v1/rules`, a repository's with
 * `PUT /v1/repositories/{id}/files/.moebius/rules.md`; the server checks both and says which line is wrong.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Link } from "react-router";
import { startChat } from "../../api/calls/chat";
import { rules, saveRepositoryRules, saveRules } from "../../api/calls/rules";
import { keys } from "../../api/keys";
import { Tag } from "../../ui/chat/Tag";
import { Button } from "../../ui/components/Button";
import { Loader } from "../../ui/components/Loader";
import { Body, Box, Notice, PageHeader, sayError, TextArea } from "../kit/Kit";
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
      breadcrumb="Build"
      title="Rules"
      context="What a run must not do. Every tool call is checked against these first; a refusal names the rule."
    />
  );
  if (data.isPending) {
    return (
      <>
        {header}
        <Loader message="Reading the rules…" className="py-double" />
      </>
    );
  }
  if (data.error || !data.data) {
    return (
      <>
        {header}
        <Notice tone="error">{sayError(data.error, "Could not read the rules.")}</Notice>
      </>
    );
  }
  const view = data.data;
  const prose = view.prose;
  const where =
    prose.source === "harness"
      ? "rules.md of the harness configuration repository"
      : prose.source === "file"
        ? "the rules_file of moebius.toml"
        : "No rules file";

  return (
    <>
      {header}
      <div className="flex flex-col gap-double">
        <Box title="Built-in patterns" aside="Always on · checked first, no model asked">
          {view.patterns.map((pattern) => (
            <div
              key={pattern.name}
              className="flex flex-wrap items-center gap-plusfifty rounded-md border bg-recessed px-plusfifty py-base"
            >
              <Tag>Pattern</Tag>
              <span className="w-[210px] font-mono text-sm text-high">{pattern.name}</span>
              <span className="min-w-[200px] flex-1 break-words text-xs text-normal">
                {pattern.description}
              </span>
              <Tag tone={(view.refusals[pattern.name] ?? 0) > 0 ? "red" : "plain"}>
                {refusalsLabel(view.refusals, pattern.name)}
              </Tag>
            </div>
          ))}
        </Box>

        <Box
          title="Rules in prose"
          aside={
            <>
              {where}
              {prose.editable && !byAgent && (
                <Button size="sm" variant="outline" onClick={() => setByAgent(true)}>
                  Let the Moebius agent write them
                </Button>
              )}
            </>
          }
        >
          {prose.editable && byAgent && (
            <div className="flex flex-col gap-base">
              <TextArea
                aria-label="What should be allowed or forbidden"
                placeholder="What should be allowed or forbidden. The Moebius agent changes the rules and saves them."
                rows={4}
                value={wish}
                onChange={(event) => setWish(event.target.value)}
              />
              <div className="flex gap-base">
                <Button
                  size="sm"
                  disabled={ask.isPending || wish.trim() === ""}
                  onClick={() => ask.mutate()}
                >
                  Ask the agent
                </Button>
                <Button size="sm" variant="outline" onClick={() => setByAgent(false)}>
                  Cancel
                </Button>
              </div>
              {ask.error && <Notice tone="error">{sayError(ask.error, "Could not ask.")}</Notice>}
            </div>
          )}
          {ask.isSuccess && (
            <Notice>
              The Moebius agent is on it.{" "}
              <Link className="text-accent hover:underline" to={`/chat/${ask.data.id}`}>
                Open the chat
              </Link>
            </Notice>
          )}
          {prose.error && <Notice tone="error">{prose.error}</Notice>}
          <RuleList
            scope="Server"
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
        </Box>

        {view.repositories.map((repository) => (
          <Box
            key={repository.repository_id}
            title={repository.repository}
            aside="Added to the server's rules; cannot weaken them"
          >
            {repository.error && <Notice tone="error">{repository.error}</Notice>}
            <RuleList
              scope="Repo"
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
          </Box>
        ))}
      </div>
    </>
  );
}
