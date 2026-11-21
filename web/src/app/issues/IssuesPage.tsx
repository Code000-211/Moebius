/**
 * The Issues page: the cards of every project in one list, filtered by project, kind, state and by any field of any
 * kind, and ordered by a field. It is the view across projects: a field defined once for a kind is the same filter
 * whichever project a card is in. What is set is kept in the address, as the board's filters are, and the server does
 * the filtering (`GET /v1/issues`); the values a filter offers come from the cards there are.
 */
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { listIssues, listKinds } from "../../api/calls/kinds";
import { people as loadPeople } from "../../api/calls/people";
import { listProjects } from "../../api/calls/projects";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import { FilterBar } from "../../ui/filters/FilterBar";
import { readDisplay, readFilters } from "../../ui/filters/model";
import { useQueryParams } from "../../ui/filters/useQueryParams";
import { Empty } from "../../ui/page/Empty";
import { ListRow } from "../../ui/page/ListRow";
import { Notice } from "../../ui/page/Notice";
import { PageHeader } from "../../ui/page/PageHeader";
import { SkeletonList } from "../../ui/page/Skeleton";
import { useFillHeight } from "../../ui/page/useFillHeight";
import { VirtualList } from "../../ui/page/VirtualList";
import { iconOf } from "../board/CardWindow";
import { useOpenCard } from "../board/CardWindowHost";
import { cardKey } from "../board/cardKey";
import { isHeld, isLive } from "../board/model";
import { rowAge } from "../board/rowAge";
import { fieldsLine, issueDisplayChoices, issueFilterFields, issueQuery } from "../kinds/model";
import { NewIssueButton } from "./NewIssueButton";

export function IssuesPage() {
  const [params, setQuery] = useQueryParams();
  const openCard = useOpenCard();
  const fill = useFillHeight();
  const [selected, setSelected] = useState<string | null>(null);
  const now = Date.now() / 1000;
  const kinds = useQuery({ queryKey: keys.issueKinds, queryFn: listKinds });
  const projects = useQuery({ queryKey: keys.projects, queryFn: listProjects });
  const people = useQuery({ queryKey: keys.people, queryFn: loadPeople });
  // Every card, which is where the values a filter offers come from; the cards shown are the ones the query picks.
  const all = useQuery({ queryKey: keys.issues, queryFn: () => listIssues(new URLSearchParams()) });
  const query = issueQuery(params);
  const shown = useQuery({
    queryKey: keys.issuesFor(query.toString()),
    queryFn: () => listIssues(query),
    // The list stays while the next one is read, so a changed filter does not blank the page.
    placeholderData: keepPreviousData,
  });

  const kindList = kinds.data ?? [];
  const projectList = projects.data ?? [];
  const choices = issueDisplayChoices(kindList);
  const display = readDisplay(params, choices);
  const fields = issueFilterFields(
    kindList,
    projectList,
    all.data ?? [],
    people.data ?? [],
    Object.fromEntries(params),
  );
  const values = readFilters(params, fields);
  return (
    <div data-testid="issues-page" className="flex flex-col gap-4">
      <PageHeader
        title="Issues"
        actions={<NewIssueButton />}
        context="The cards of every project. Filter by a kind's fields, whichever project a card is in."
      />
      <FilterBar
        label="Filter the issues"
        fields={fields}
        values={values}
        onFilter={(key, value) => setQuery({ [key]: value })}
        onClear={() => setQuery(Object.fromEntries(fields.map((f) => [f.key, ""])))}
        display={{
          choices,
          value: display,
          onChange: (next) => setQuery({ sort: next.sort }),
        }}
      />
      {shown.isPending && <SkeletonList rows={6} message="Reading the issues" />}
      {shown.isError && (
        <Notice tone="error">
          {sayError(shown.error, "Could not read the issues. Try again.")}
        </Notice>
      )}
      {shown.data?.length === 0 && (
        <Empty title="No issue matches" body="Take a filter off to see more of them." />
      )}
      {shown.data && shown.data.length > 0 && (
        <div ref={fill.ref} style={{ height: fill.height }}>
          <VirtualList
            label="Issues"
            items={shown.data.map((card) => ({
              type: "row" as const,
              key: String(card.id),
              data: card,
            }))}
            selectedKey={selected}
            onSelect={setSelected}
            onOpen={(key) => openCard(Number(key))}
            className="h-full outline-none"
            renderRow={(card, { at, onClick, selected: chosen }) => (
              <ListRow
                at={at}
                status={iconOf(card, isHeld(card) ? "held" : isLive(card) ? "live" : "idle")}
                id={cardKey(card)}
                title={card.title}
                reason={card.kind ? `${card.kind} ${fieldsLine(card)}`.trim() : fieldsLine(card)}
                age={rowAge(card.created_at, now)}
                selected={chosen}
                onClick={(event) => {
                  onClick(event);
                  openCard(card.id);
                }}
              />
            )}
          />
        </div>
      )}
    </div>
  );
}
