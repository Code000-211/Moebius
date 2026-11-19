/**
 * The Following property of a card in the dock: whether the person signed in follows it, and the button that makes
 * them follow it or stop (ADR 0018). A person follows the cards they wrote and are given until they stop; any other
 * card they follow by choice. A change to a followed card is a notice under the Inbox's Mentions tab.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { follow, followers } from "../../api/calls/notices";
import { getSession } from "../../api/calls/session";
import { keys } from "../../api/keys";
import { Button } from "../../ui/components/Button";
import { Property } from "../../ui/page/Properties";

export function FollowProperty({ card }: { card: number }) {
  const queryClient = useQueryClient();
  const session = useQuery({ queryKey: keys.session, queryFn: getSession });
  const list = useQuery({ queryKey: keys.followers(card), queryFn: () => followers(card) });
  const change = useMutation({
    mutationFn: (following: boolean) => follow(card, following),
    onSuccess: (next) => queryClient.setQueryData(keys.followers(card), next),
  });
  const me = session.data?.person?.handle;
  // The server's own token has no person to follow as.
  if (me === undefined || list.data === undefined) return null;
  const following = list.data.some((handle) => handle.toLowerCase() === me.toLowerCase());
  return (
    <Property name="Following">
      <span className="flex items-center gap-2">
        <span className={following ? "text-text" : "text-secondary"}>
          {following ? "You follow it" : "Not following"}
        </span>
        <Button
          variant="ghost"
          size="sm"
          disabled={change.isPending}
          onClick={() => change.mutate(!following)}
        >
          {following ? "Stop following" : "Follow"}
        </Button>
      </span>
    </Property>
  );
}
