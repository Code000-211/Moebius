/**
 * The Rules page: the built-in patterns and what each refused this week, the server's rules in prose and the rules
 * each repository adds, as one list of 36px rows grouped by set. A row opens its rule in the editor window
 * (`RuleList`, `ui/components/EditorWindow.tsx`); "Write with agent" asks the
 * Moebius agent in a chat instead. The server's rules are saved with `PUT /v1/rules`, a repository's with
 * `PUT /v1/repositories/{id}/settings/rules.md`; the server checks both and says which line is wrong.
 *
 * The page reads from what cannot be changed to what a person can: the patterns first, then the server's own rules,
 * then one group per repository. The shared filter bar narrows it to one scope or one project (`filters.ts`), and keeps
 * that in the address.
 */

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Link } from "react-router";
import { startChat } from "../../api/calls/chat";
import { rules, saveRepositoryRules, saveRules } from "../../api/calls/rules";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import { Button } from "../../ui/components/Button";
import { FilterBar } from "../../ui/filters/FilterBar";
import { readFilters } from "../../ui/filters/model";
import { useQueryParams } from "../../ui/filters/useQueryParams";
import { SparkleIcon } from "../../ui/icons";
import { Field, TextArea } from "../../ui/page/Field";
import { GroupRow } from "../../ui/page/ListRow";
import { Notice } from "../../ui/page/Notice";
import { PageHeader } from "../../ui/page/PageHeader";
import { Reading, SkeletonCard } from "../../ui/page/Skeleton";
import { agentRequest } from "./file";
import { FILTER_KEYS, filterFields, visible } from "./filters";
import { RuleList, RuleRow, refused } from "./RuleList";

export function RulesPage() {
  const queryClient = useQueryClient();
  const data = useQuery({ queryKey: keys.rules, queryFn: rules });
  const refresh = () => queryClient.invalidateQueries({ queryKey: keys.rules });
  const [params, setQuery] = useQueryParams();
  const [byAgent, setByAgent] = useState(false);
  const [wish, setWish] = useState("");
  const ask = useMutation({
    mutationFn: () => startChat({ agent: "Moebius", message: agentRequest(wish.trim()) }),
    onSuccess: () => {
      setByAgent(false);
      setWish("");
    },
  });

  const header = <PageHeader title="Rules" />;
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
  const fields = filterFields(view);
  const values = readFilters(params, fields);
  const show = visible(values);
  const shownRepositories = view.repositories.filter((each) => show.project(each.repository_id));

  return (
    <div className="flex h-full min-h-0 flex-col">
      {header}
      {/* The button's own padding is taken back, so that "Filter" starts where the rows below it start. */}
      <div className="-ml-2.5">
        <FilterBar
          label="Filter the rules"
          fields={fields}
          values={values}
          onFilter={(key, value) => setQuery({ [key]: value })}
          onClear={() => setQuery(Object.fromEntries(FILTER_KEYS.map((key) => [key, ""])))}
        />
      </div>
      {!show.patterns && !show.server && shownRepositories.length === 0 && (
        <Notice>No rule is in that scope. Clear the filters to see them all.</Notice>
      )}
      <div className="min-h-0 flex-1 overflow-y-auto">
        {show.patterns && (
          <section aria-label="Built-in patterns">
            <GroupRow name="Built-in patterns · always on" />
            {view.patterns.map((pattern) => (
              <RuleRow
                key={pattern.name}
                name={pattern.name}
                text={pattern.description}
                state={refused(view.refusals, pattern.name)}
              />
            ))}
          </section>
        )}
        {show.server && (
          <section aria-label="Rules in prose">
            <GroupRow name={prose.editable ? "Server rules" : "Server rules · read only"} />
            {prose.editable && byAgent && (
              <div className="flex flex-col gap-4 border-b border-hairline p-4">
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
              <div className="flex items-center gap-3 border-b border-hairline px-4 py-2">
                <span className="min-w-0 flex-1 text-base text-text">
                  The Moebius agent is writing the rules.
                </span>
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
            {prose.editable && !byAgent && (
              <RuleRow
                name="Write with agent"
                icon={<SparkleIcon className="size-4 shrink-0 text-muted" aria-hidden="true" />}
                onOpen={() => setByAgent(true)}
              />
            )}
          </section>
        )}
        {shownRepositories.map((repository) => (
          <section key={repository.repository_id} aria-label={repository.repository}>
            <GroupRow name={repository.repository} />
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
          </section>
        ))}
      </div>
    </div>
  );
}
