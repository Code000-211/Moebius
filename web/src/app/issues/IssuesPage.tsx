/**
 * The Issues page: the cards of every project in one list, filtered by project, kind, state and by any field of any
 * kind, and ordered by a field. It is the view across projects: a field defined once for a kind is the same filter
 * whichever project a card is in. What is set is kept in the address, as the board's filters are, and the server does
 * the filtering (`GET /v1/issues`); the values a filter offers come from the cards there are.
 */
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { Link } from "react-router";
import { listIssues, listKinds } from "../../api/calls/kinds";
import { people as loadPeople } from "../../api/calls/people";
import { listProjects } from "../../api/calls/projects";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import { FilterBar } from "../../ui/filters/FilterBar";
import { readDisplay, readFilters } from "../../ui/filters/model";
import { useQueryParams } from "../../ui/filters/useQueryParams";
import { Box } from "../../ui/page/Box";
import { Empty } from "../../ui/page/Empty";
import { Notice } from "../../ui/page/Notice";
import { PageHeader } from "../../ui/page/PageHeader";
import { SkeletonLines } from "../../ui/page/Skeleton";
import { cardKey } from "../board/cardKey";
import { fieldsLine, issueDisplayChoices, issueFilterFields, issueQuery } from "../kinds/model";
import { boardPath } from "../pages";

export function IssuesPage() {
  const [params, setQuery] = useQueryParams();
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
      {shown.isPending && <SkeletonLines lines={4} message="Reading the issues" />}
      {shown.isError && (
        <Notice tone="error">
          {sayError(shown.error, "Could not read the issues. Try again.")}
        </Notice>
      )}
      {shown.data?.length === 0 && (
        <Empty title="No issue matches" body="Take a filter off to see more of them." />
      )}
      {shown.data && shown.data.length > 0 && (
        <Box title={`${shown.data.length} issues`} padded={false}>
          <ul className="m-0 flex list-none flex-col p-0">
            {shown.data.map((card) => (
              <li
                key={card.id}
                data-testid={`issue-${card.id}`}
                className="flex flex-wrap items-baseline gap-x-3 border-b border-hairline px-4 py-2 last:border-b-0"
              >
                <span className="text-sm text-secondary tabular-nums">{cardKey(card)}</span>
                <Link
                  className="min-w-0 flex-1 text-text underline decoration-hairline-strong underline-offset-4 hover:decoration-current"
                  to={boardPath(card.repository_id)}
                >
                  {card.title}
                </Link>
                {card.kind && <span className="text-sm text-secondary">{card.kind}</span>}
                <span className="text-sm text-secondary">{fieldsLine(card)}</span>
              </li>
            ))}
          </ul>
        </Box>
      )}
    </div>
  );
}
