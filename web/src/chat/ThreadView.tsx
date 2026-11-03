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
import { Button } from "../ui/components/Button";
import { Empty } from "../ui/page/Empty";
import { useScrollFades } from "../ui/page/ScrollFades";
import { Reading, Skeleton, SkeletonThread } from "../ui/page/Skeleton";
import type { Entry } from "./conversation";
import {
  afterScroll,
  afterSend,
  afterToggle,
  byPerson,
  prepended,
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
  const given = useRef(ref);
  given.current = ref;
  const hold = useCallback(
    (element: HTMLDivElement | null) => {
      fades.box.current = element;
      const target = given.current;
      if (typeof target === "function") target(element);
      else if (target) target.current = element;
    },
    [fades.box],
  );
  return (
    <div {...rest} ref={hold} onScroll={fades.measure} style={{ ...style, ...fades.style }}>
      {children}
    </div>
  );
}

/**
 * What the row above the oldest turn can say: whether a page of the chat is on its way, why it could not come, and
 * how to ask for it again. It comes to the list as its context, so the components of the list are named once rather
 * than rebuilt each render.
 */
interface Older {
  /** Whether the chat goes on above the oldest turn drawn, so asking again would bring something. */
  hasOlder: boolean;
  olderLoading: boolean;
  olderError: string | null;
  loadOlder?: () => void;
}

/**
 * Where the list of turns starts counting. It counts down as turns arrive above what is drawn, and a chat has no
 * known length, so it starts far from zero: a list whose count goes below zero stops working altogether.
 */
const FIRST_INDEX = 1_000_000;

/**
 * The row above the oldest turn, which says what is happening above it rather than asking to be pressed. Reaching the
 * top of the conversation is what reads the rest (see `startReached` below), so a button offering the same thing is a
 * step in the middle of a gesture that already asked for it — and, worse, a promise the page may not be able to keep.
 * The row is drawn only while that read is on its way, when it draws the shape of the turns coming, or when it failed
 * and asking again is a button with something to do.
 */
function OlderRow({ context }: { context: Older }) {
  const failed = context.olderError !== null && context.hasOlder;
  if (!context.olderLoading && !failed) return null;
  return (
    <div className="flex min-h-14 items-center justify-center px-4 pb-3" data-testid="older">
      {context.olderLoading ? (
        <Reading
          message="Reading earlier messages"
          className="flex w-full max-w-prose flex-col gap-2"
        >
          <Skeleton className="h-4 w-1/4" />
          <Skeleton className="w-full" />
          <Skeleton className="w-3/4" />
        </Reading>
      ) : (
        <span className="flex items-center gap-3">
          <span className="text-sm text-secondary">{context.olderError}</span>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={() => context.loadOlder?.()}
            data-testid="retry-older"
          >
            Try again
          </Button>
        </span>
      )}
    </div>
  );
}

const COMPONENTS = { Scroller: FadedScroller, Header: OlderRow };

interface ThreadViewProps {
  entries: Entry[];
  /** Whether the agent works now. */
  working: boolean;
  /** How many messages the person has sent from this page; a new one follows the conversation again. */
  sent: number;
  /**
   * The events are still being read, so the empty place is not yet a fact about the chat: the turns that are coming
   * are drawn as bars instead, so the page fills in place rather than showing a card that is about to be thrown away.
   */
  loading?: boolean;
  /** What is being read while `loading`, said once out loud: "Reading the chat". */
  loadingMessage?: string;
  /** What belongs here while it is empty, said in a sentence. Leave it out while the page is still loading. */
  empty?: string;
  /** What the empty place is called; "Nothing said yet" unless the page says otherwise. */
  emptyTitle?: string;
  /** What a person can press to start, while the place is empty: an empty chat that offers nothing asks to be read rather than used. */
  emptyAction?: ReactNode;
  /**
   * Drawn in the floating chat's dock rather than on a page: one row of turns in less room, so the rhythm tightens and
   * the empty place sits on the dock's own surface instead of in a card inside it.
   */
  compact?: boolean;
  /** Whether the chat goes on above the oldest turn drawn. A long one opens at its end; scrolling to its top reads the rest. */
  hasOlder?: boolean;
  /** Whether the messages above are being read. */
  olderLoading?: boolean;
  /** Why the messages above could not be read, if they could not. */
  olderError?: string | null;
  /** Reads the messages above the oldest turn drawn. */
  loadOlder?: () => void;
}

export function ThreadView({
  entries,
  working,
  sent,
  loading = false,
  loadingMessage,
  empty,
  emptyTitle = "Nothing said yet",
  emptyAction,
  compact = false,
  hasOlder = false,
  olderLoading = false,
  olderError = null,
  loadOlder,
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
  /**
   * Where the list starts counting from, and the turn it counted from before. Older turns arrive above what is drawn,
   * and a list that counted from zero would move the turn a person is reading down by exactly what was added over it,
   * so the number falls as turns are added and that turn keeps its place. Once the turn it counted from is gone, what
   * arrived is another conversation, and the count starts again.
   */
  const [anchor, setAnchor] = useState({ base: FIRST_INDEX, first: null as number | null });
  const lastGesture = useRef<number | null>(null);
  const repins = useRef(0);

  const first = turns[0]?.from ?? null;

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

  if (first !== anchor.first) {
    const above = first === null ? null : prepended(turns, anchor.first);
    setAnchor(above === null ? { base: FIRST_INDEX, first } : { base: anchor.base - above, first });
  }

  // Reaching the top of the list is how a person asks for what is above it. The list opens at its end, so its top is
  // only reached by someone who scrolled up: following an answer is never interrupted by a page nobody asked for.
  const reachedStart = useCallback(() => {
    if (state.current.follow) return;
    loadOlder?.();
  }, [loadOlder]);

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
    if (loading) {
      return (
        <div className="p-4" data-testid="thread-loading">
          <SkeletonThread message={loadingMessage} />
        </div>
      );
    }
    return (
      <div className="p-4" data-testid="empty-thread">
        <Empty title={emptyTitle} body={empty} action={emptyAction} plain={compact} />
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
      firstItemIndex={anchor.base}
      startReached={reachedStart}
      context={{ hasOlder, olderLoading, olderError, loadOlder }}
      increaseViewportBy={400}
      components={COMPONENTS}
      data-testid="thread"
      style={{ height: "100%" }}
      itemContent={(_, turn) => (
        <div className={compact ? "pb-3" : "pb-6"}>
          <Turn turn={turn} unfolded={unfolded} onToggle={toggle} compact={compact} />
        </div>
      )}
    />
  );
}
