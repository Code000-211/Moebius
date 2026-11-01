/**
 * The Moebius mark: the strip the app is named after. It draws in the current colour, so it reads on the sidebar, on a
 * card or on a dialog without carrying a colour of its own. `public/favicon.svg` is the same path with the colour
 * written into it, because a tab icon is loaded before any stylesheet and cannot ask.
 */
export function Mark({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 22 16" aria-hidden="true" className={className}>
      <path
        d="M2 8c0-3 2-6 5-6 4 0 4 12 8 12 3 0 5-3 5-6s-2-6-5-6c-4 0-4 12-8 12-3 0-5-3-5-6z"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
      />
    </svg>
  );
}
