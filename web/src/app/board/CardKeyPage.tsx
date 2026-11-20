/**
 * The address of a card by its key, `/card/WEB-24`: it opens the card on its board, the way a link in a comment, the
 * palette and the command line name it. It asks the server for the card by its key, not the snapshot, and reads the
 * snapshot and what the card's page shows at the same time, so the board it goes to finds them under way. A key no card
 * has shows a not-found page.
 */
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { Navigate, useParams } from "react-router";
import { cardByKey, snapshot } from "../../api/calls/board";
import { keys } from "../../api/keys";
import { Empty } from "../../ui/page/Empty";
import { PageHeader } from "../../ui/page/PageHeader";
import { boardPath } from "../pages";
import { parseCardKey } from "./cardKey";
import { prefetchCard } from "./prefetchCard";

export function CardKeyPage() {
  const { key = "" } = useParams();
  const named = parseCardKey(key);
  const queryClient = useQueryClient();
  const canonical = named ? `${named.key}-${named.number}` : "";
  const found = useQuery({
    queryKey: ["card", "by-key", canonical],
    queryFn: () => cardByKey(canonical),
    enabled: named !== null,
  });
  // The board this goes to needs the snapshot: it is read now, beside the card, not after the redirect.
  useEffect(() => {
    void queryClient.prefetchQuery({ queryKey: keys.snapshot, queryFn: () => snapshot() });
  }, [queryClient]);
  const card = found.data;
  useEffect(() => {
    if (card) prefetchCard(queryClient, card.id, { project: card.project_id, column: card.column });
  }, [queryClient, card]);
  if (card) return <Navigate to={`${boardPath(card.repository_id)}?card=${card.id}`} replace />;
  if (named && found.isPending) return <PageHeader title={key.toUpperCase()} />;
  return (
    <>
      <PageHeader title={key.toUpperCase()} />
      <Empty title="There is no such card" body="No card has this key; check the project's key." />
    </>
  );
}
