/**
 * The address of a card by its key, `/card/WEB-24`: it opens the card on its board, the way a link in a comment, the
 * palette and the command line name it. It waits for the snapshot and shows a not-found page when no card has the key.
 */
import { useQuery } from "@tanstack/react-query";
import { Navigate, useParams } from "react-router";
import { snapshot } from "../../api/calls/board";
import { keys } from "../../api/keys";
import { Empty } from "../../ui/page/Empty";
import { PageHeader } from "../../ui/page/PageHeader";
import { boardPath } from "../pages";
import { parseCardKey } from "./cardKey";

export function CardKeyPage() {
  const { key = "" } = useParams();
  const named = parseCardKey(key);
  const { data, isPending } = useQuery({ queryKey: keys.snapshot, queryFn: snapshot });
  if (named && isPending) return <PageHeader title={key.toUpperCase()} />;
  for (const board of data?.boards ?? []) {
    const card = board.cards.find(
      (c) => c.project_key === named?.key && c.project_number === named?.number,
    );
    if (card) return <Navigate to={`${boardPath(board.repository.id)}?card=${card.id}`} replace />;
  }
  return (
    <>
      <PageHeader title={key.toUpperCase()} />
      <Empty title="There is no such card" body="No card has this key; check the project's key." />
    </>
  );
}
