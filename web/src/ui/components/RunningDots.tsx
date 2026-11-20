// From BloopAI/vibe-kanban packages/ui/src/components/RunningDots.tsx (Apache-2.0), changed: the label is for screen
// readers (the original was silent); the mark is one still dot in the colour of work in progress.
/**
 * That a run is working, as one still dot in the colour of work in progress. The word is always beside it — the card's
 * badge of "Live", the dock's line of the run — so the mark says where and the word says what it means. It does not
 * move: the running ring is the only thing in the app that may.
 */
export function RunningDots() {
  return (
    <span role="img" aria-label="Running" className="flex shrink-0 items-center">
      <span className="size-2 rounded-full bg-progress" />
    </span>
  );
}
