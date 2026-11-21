/**
 * The page of a card by its key, `/card/WEB-24`: "Open as page" of the card window, and the address a link in a
 * comment, the palette and the command line name. It shows the same content as the window (`CardContent`) at full width
 * with the same actions, without a window or a scrim, and the way back to its board at the left of its title. It asks the server for the card by its key, not the snapshot, and
 * reads the snapshot and what the card shows at the same time. A key no card has shows a not-found page.
 */
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { cardByKey, snapshot } from "../../api/calls/board";
import { keys } from "../../api/keys";
import { Button } from "../../ui/components/Button";
import { Tooltip } from "../../ui/components/Tooltip";
import { BackIcon } from "../../ui/icons";
import { Empty } from "../../ui/page/Empty";
import { PageHeader } from "../../ui/page/PageHeader";
import { boardPath } from "../pages";
import { CardContent } from "./CardWindow";
import { cardPagePath, parseCardKey } from "./cardKey";
import { inboxOf, mergeStoppedOf } from "./model";
import { prefetchCard } from "./prefetchCard";
import { useCardActions } from "./useCardActions";

export function CardKeyPage() {
  const { key = "" } = useParams();
  const named = parseCardKey(key);
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const canonical = named ? `${named.key}-${named.number}` : "";
  const found = useQuery({
    queryKey: ["card", "by-key", canonical],
    queryFn: () => cardByKey(canonical),
    enabled: named !== null,
  });
  const data = useQuery({ queryKey: keys.snapshot, queryFn: () => snapshot() });
  const found_ = found.data;
  useEffect(() => {
    if (found_)
      prefetchCard(queryClient, found_.id, { project: found_.project_id, column: found_.column });
  }, [queryClient, found_]);
  // A card deleted from its own page leaves for the board it was on.
  const actions = useCardActions(() => navigate(boardPath(found_?.repository_id ?? 0)));

  const boards = data.data?.boards;
  const inbox = data.data?.inbox;
  const board = boards?.find((b) => b.cards.some((c) => c.id === found_?.id));
  const card = board?.cards.find((c) => c.id === found_?.id);

  if (board && card && inbox) {
    return (
      <div className="mx-auto flex w-full max-w-3xl flex-col">
        <CardContent
          board={board}
          card={card}
          item={inboxOf(inbox, card)}
          stopped={mergeStoppedOf(inbox, card)}
          boards={boards}
          busy={actions.busy}
          askFor={actions.askFor}
          onAskAnswered={actions.clearReject}
          onDecide={actions.decide}
          menu={actions.menu}
          onOpenCard={(next) => navigate(cardPagePath(next))}
          back={
            <Tooltip content="Back to the board" side="bottom">
              <Button asChild variant="icon" aria-label="Back to the board">
                <Link to={boardPath(card.repository_id)}>
                  <BackIcon className="size-5" />
                </Link>
              </Button>
            </Tooltip>
          }
          scrolls={false}
        />
        {actions.focus(card, inboxOf(inbox, card))}
        {actions.dialogs(board)}
      </div>
    );
  }
  const waiting = (named && found.isPending) || (found_ && data.isPending);
  if (waiting) return <PageHeader title={key.toUpperCase()} />;
  return (
    <>
      <PageHeader title={key.toUpperCase()} />
      <Empty title="There is no such card" body="No card has this key; check the project's key." />
    </>
  );
}
