/**
 * The conversation of a chat as a virtualised list of turns. It follows the answer while it streams: the list scrolls
 * to its end as it grows, unless the person scrolled up (see `follow.ts`); reaching the end again, or sending a message
 * (`sent` counts them), follows again.
 */
import {
  type ReactNode,
  useCallback,
  useDeferredValue,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { type ScrollerProps, Virtuoso, type VirtuosoHandle } from "react-virtuoso";
import { Turn } from "../ui/chat/Turn";
import { Empty } from "../ui/page/Empty";
import { useScrollFades } from "../ui/page/ScrollFades";
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

/**
 * The scroller of the list, with the edges that have more behind them faded out. A conversation is read from its end,
 * so the text cut off at the top is always the part just left behind; fading it lets the turns blend into the heading
 * above rather than ending mid-word. The object that names it is made once, below, because a new one each render would
 * make the list build its scroller again.
 */
function FadedScroller({ children, style, ref, ...rest }: ScrollerProps) {
  const fades = useScrollFades<HTMLDivElement>();
  return (
    <div
      {...rest}
      ref={(element) => {
        fades.box.current = element;
        if (typeof ref === "function") ref(element);
        else if (ref) ref.current = element;
      }}
      onScroll={fades.measure}
      style={{ ...style, ...fades.style }}
    >
      {children}
    </div>
  );
}

const COMPONENTS = { Scroller: FadedScroller };

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
  // An answer arrives piece by piece, and the person may be typing while it does: the words shown trail the events by
  // a render rather than cutting the keystroke off, and a long conversation groups once per batch instead of per piece.
  const shown = useDeferredValue(entries);
  const turns = useMemo(() => groupTurns(shown, working), [shown, working]);
  const [unfolded, setUnfolded] = useState<ReadonlySet<string>>(new Set());
  const list = useRef<VirtuosoHandle>(null);
  const scroller = useRef<HTMLElement | null>(null);
  /** Takes the listeners of the scroller currently held, so attaching a new one leaves nothing behind. */
  const leave = useRef<(() => void) | null>(null);
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
      leave.current?.();
      leave.current = null;
      scroller.current = element instanceof HTMLElement ? element : null;
      const current = scroller.current;
      if (!current) return;
      const gesture = () => {
        lastGesture.current = performance.now();
      };
      current.onscroll = () => onScroll(current);
      current.onkeydown = gesture;
      // A press on the scroller itself is the scrollbar's; a press on a turn is not a scroll.
      current.onpointerdown = (event) => {
        if (event.target === current) gesture();
      };
      // Wheel and touch are listened to passively: a listener that could cancel them makes the browser wait for it
      // before the page moves, and this one only notes that a hand moved.
      current.addEventListener("wheel", gesture, { passive: true });
      current.addEventListener("touchmove", gesture, { passive: true });
      leave.current = () => {
        current.onscroll = null;
        current.onkeydown = null;
        current.onpointerdown = null;
        current.removeEventListener("wheel", gesture);
        current.removeEventListener("touchmove", gesture);
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
      components={COMPONENTS}
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
