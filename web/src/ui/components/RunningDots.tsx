// From BloopAI/vibe-kanban packages/ui/src/components/RunningDots.tsx (Apache-2.0), changed: the label is for screen
// readers (the original was silent); the dots are the colour of work in progress; the animation names are defined in
// styles/tailwind.css.
/**
 * That a run is working, as three dots that breathe. The word is always beside it — the card's badge of "Live", the
 * dock's line of the run — so the motion marks the change and the word says what it means.
 */
export function RunningDots() {
  return (
    <span role="img" aria-label="Running" className="flex shrink-0 items-center gap-1">
      <span className="size-2 rounded-full bg-progress animate-running-dot-1" />
      <span className="size-2 rounded-full bg-progress animate-running-dot-2" />
      <span className="size-2 rounded-full bg-progress animate-running-dot-3" />
    </span>
  );
}
