/**
 * The conversation of a chat as a virtualised list of turns. It follows the answer while it streams: the list scrolls
 * to its end as it grows, unless the person scrolled up (see `follow.ts`); reaching the end again, or sending a message
 * (`sent` counts them), follows again.
 */
import { type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Virtuoso, type VirtuosoHandle } from "react-virtuoso";
import { Turn } from "../ui/chat/Turn";
import { Empty } from "../ui/page/Empty";
import type { Entry } from "./conversation";
import {
  afterScroll,
  afterSend,
  afterToggle,
  byPerson,
  type ScrollState,
  shouldRepin,
} from "./follow";
import { groupTurns } from "./turns";

interface ThreadViewProps {
  entries: Entry[];
  /** Whether the agent works now. */
  working: boolean;
  /** How many messages the person has sent from this page; a new one follows the conversation again. */
  sent: number;
  /** What belongs here while it is empty, said in a sentence. Leave it out while the page is still loading. */
  empty?: string;
  /** What the empty place is called; "Nothing said yet" unless the page says otherwise. */
  emptyTitle?: string;
  /** What a person can press to start, while the place is empty: an empty chat that offers nothing asks to be read rather than used. */
  emptyAction?: ReactNode;
}

export function ThreadView({
  entries,
  working,
  sent,
  empty,
  emptyTitle = "Nothing said yet",
  emptyAction,
}: ThreadViewProps) {
  const turns = useMemo(() => groupTurns(entries, working), [entries, working]);
  const [unfolded, setUnfolded] = useState<ReadonlySet<string>>(new Set());
  const list = useRef<VirtuosoHandle>(null);
  const scroller = useRef<HTMLElement | null>(null);
  const state = useRef<ScrollState>({ follow: true, top: 0 });
  const lastGesture = useRef<number | null>(null);
  const repins = useRef(0);

  const toggle = useCallback((key: string) => {
    // What is opened is read where it is, so the list lets go of its end rather than scrolling the new text away.
    state.current = afterToggle(state.current);
    setUnfolded((held) => {
      const next = new Set(held);
      if (!next.delete(key)) next.add(key);
      return next;
    });
  }, []);

  const toEnd = useCallback(() => {
    const element = scroller.current;
    if (element && state.current.follow) element.scrollTop = element.scrollHeight;
  }, []);

  const onScroll = useCallback(
    (element: HTMLElement) => {
      const scrolled = {
        top: element.scrollTop,
        height: element.scrollHeight,
        viewport: element.clientHeight,
      };
      const before = state.current;
      state.current = afterScroll(
        before,
        scrolled,
        byPerson(lastGesture.current, performance.now()),
      );
      // The list moved a following view away from the end (it corrects its position while it measures): put it back,
      // a few times at most so that the two never fight.
      if (shouldRepin(state.current, scrolled) && repins.current < 5) {
        repins.current += 1;
        requestAnimationFrame(toEnd);
      } else if (!shouldRepin(state.current, scrolled)) {
        repins.current = 0;
      }
    },
    [toEnd],
  );

  const attach = useCallback(
    (element: HTMLElement | Window | null) => {
      const previous = scroller.current;
      if (previous) {
        previous.onscroll = null;
        previous.onwheel = null;
        previous.ontouchmove = null;
        previous.onkeydown = null;
        previous.onpointerdown = null;
      }
      scroller.current = element instanceof HTMLElement ? element : null;
      const current = scroller.current;
      if (!current) return;
      const gesture = () => {
        lastGesture.current = performance.now();
      };
      current.onscroll = () => onScroll(current);
      current.onwheel = gesture;
      current.ontouchmove = gesture;
      current.onkeydown = gesture;
      // A press on the scroller itself is the scrollbar's; a press on a turn is not a scroll.
      current.onpointerdown = (event) => {
        if (event.target === current) gesture();
      };
    },
    [onScroll],
  );

  // A message just sent is followed to the end, wherever the person had scrolled.
  // biome-ignore lint/correctness/useExhaustiveDependencies: it runs when `sent` counts one more message
  useEffect(() => {
    if (sent === 0) return;
    state.current = afterSend(state.current);
    toEnd();
  }, [sent]);

  // The turns grew (an answer streamed in): scroll on after the list measured them.
  // biome-ignore lint/correctness/useExhaustiveDependencies: it runs when the turns change
  useEffect(() => {
    const frame = requestAnimationFrame(toEnd);
    return () => cancelAnimationFrame(frame);
  }, [turns]);

  if (turns.length === 0) {
    return (
      <div className="p-4" data-testid="empty-thread">
        <Empty title={emptyTitle} body={empty} action={emptyAction} />
      </div>
    );
  }
  return (
    <Virtuoso
      ref={list}
      data={turns}
      initialTopMostItemIndex={{ index: "LAST", align: "end" }}
      scrollerRef={attach}
      totalListHeightChanged={toEnd}
      computeItemKey={(_, turn) => turn.key}
      increaseViewportBy={400}
      data-testid="thread"
      style={{ height: "100%" }}
      itemContent={(_, turn) => (
        <div className="pb-6">
          <Turn turn={turn} unfolded={unfolded} onToggle={toggle} />
        </div>
      )}
    />
  );
}
