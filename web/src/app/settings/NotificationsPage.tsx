/**
 * The Notifications entry of Settings. The server has no notification preferences: it keeps one notice per thing that
 * concerns the person (`GET /v1/notices`) and the Inbox shows them. So this page says which kinds of notice the person
 * receives, how many are unread, and leads to the Inbox.
 */
import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router";
import { notices as readNotices } from "../../api/calls/notices";
import { keys } from "../../api/keys";
import { Button } from "../../ui/components/Button";
import { FieldRow, Rows } from "../../ui/page/Field";
import { AreaTitle } from "./parts";

const KINDS = [
  { kind: "assigned", label: "Assigned", note: "A card is given to you" },
  { kind: "mentioned", label: "Mentions", note: "Someone names you in a comment" },
  { kind: "changed", label: "Following", note: "A card you follow changes" },
] as const;

export function NotificationsPage() {
  const notices = useQuery({ queryKey: keys.notices, queryFn: readNotices });
  return (
    <div className="max-w-3xl">
      <AreaTitle
        title="Notifications"
        actions={
          <Button asChild size="sm" variant="secondary">
            <Link to="/inbox">Open Inbox</Link>
          </Button>
        }
      />
      <Rows>
        {KINDS.map((each) => (
          <FieldRow key={each.kind} label={each.label} description={each.note}>
            <span className="text-sm text-secondary tabular-nums">
              {notices.data
                ? `${notices.data.filter((n) => n.kind === each.kind && !n.read).length} unread`
                : "…"}
            </span>
          </FieldRow>
        ))}
      </Rows>
    </div>
  );
}
