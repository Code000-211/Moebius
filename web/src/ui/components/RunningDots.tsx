// From BloopAI/vibe-kanban packages/ui/src/components/RunningDots.tsx (Apache-2.0), changed: the label is for screen readers (the original was silent); the animation names are defined in styles/tailwind.css.
export function RunningDots() {
  return (
    <div role="img" aria-label="Running" className="flex items-center gap-[2px] shrink-0">
      <span className="size-dot rounded-full bg-live animate-running-dot-1" />
      <span className="size-dot rounded-full bg-live animate-running-dot-2" />
      <span className="size-dot rounded-full bg-live animate-running-dot-3" />
    </div>
  );
}
