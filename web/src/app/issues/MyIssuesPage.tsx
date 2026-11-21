/**
 * My issues: the issues given to the signed-in person, as the same virtual list of 36px rows the Inbox and Fleet use,
 * in groups by status (In progress, In review, Not started, Done). The server does the choosing
 * (`GET /v1/issues?assignee=<handle>`), so only what is the person's is read, however many issues there are; a row
 * opens the card in its window over this page (`useOpenCard`), and Enter does the same for the chosen row. A page of a
 * person nobody is signed in as, or with nothing given to them, says so in one line.
 */
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { listIssues } from "../../api/calls/kinds";
import { getSession } from "../../api/calls/session";
import { keys } from "../../api/keys";
import { ListChecksIcon } from "../../ui/icons";
import { useNow } from "../../ui/lib/useNow";
import { Empty } from "../../ui/page/Empty";
import { ListRow } from "../../ui/page/ListRow";
import { PageHeader } from "../../ui/page/PageHeader";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { VirtualList } from "../../ui/page/VirtualList";
import { useOpenCard } from "../board/CardWindowHost";
import { type MyIssue, myIssueItems } from "./model";
import { NewIssueButton } from "./NewIssueButton";

export function MyIssuesPage() {
  const session = useQuery({ queryKey: keys.session, queryFn: getSession });
  const handle = session.data?.person?.handle ?? null;
  const query = new URLSearchParams(handle === null ? {} : { assignee: handle });
  const issues = useQuery({
    queryKey: keys.issuesFor(query.toString()),
    queryFn: () => listIssues(query),
    enabled: handle !== null,
  });
  const openCard = useOpenCard();
  const now = useNow();
  const [chosen, setChosen] = useState<string | null>(null);

  const header = <PageHeader title="My issues" actions={<NewIssueButton />} />;
  const empty = (
    <Empty
      centered
      icon={<ListChecksIcon size={28} />}
      title={handle === null ? "Sign in as a person to see yours" : "Nothing is assigned to you"}
    />
  );
  if (issues.isError || session.isError) {
    const failed = issues.isError ? issues : session;
    return (
      <>
        {header}
        <ReadFailed
          error={failed.error}
          fallback="Your issues could not be read. Try again."
          busy={failed.isFetching}
          onRetry={() => void failed.refetch()}
        />
      </>
    );
  }
  const open = (key: string) => openCard(Number(key));
  return (
    <div data-testid="my-issues-page" className="flex h-full min-h-0 flex-col">
      {header}
      <VirtualList<MyIssue>
        label="My issues"
        className="-mx-4 -mb-4 -mt-4 min-h-0 flex-1 outline-none"
        items={myIssueItems(issues.data ?? [], now)}
        loading={session.isPending || (handle !== null && issues.isPending)}
        loadingMessage="Reading your issues"
        selectedKey={chosen}
        onSelect={setChosen}
        onOpen={open}
        empty={empty}
        renderRow={({ card, key, status, reason, age }, row) => (
          <ListRow
            at={row.at}
            selected={row.selected}
            status={status}
            id={key}
            title={card.title}
            reason={reason}
            age={age}
            data-key={row.key}
            onClick={(event) => {
              row.onClick(event);
              open(row.key);
            }}
          />
        )}
      />
    </div>
  );
}
