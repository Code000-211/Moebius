/**
 * Counts what React renders, for the tests that guard a page against drawing more than it must. The count is the one the
 * DevTools profiler makes: the components React performed work for in each commit, read through the hook that
 * `test-commits.ts` stands in the page. It counts the commits that happen between `watchRenders()` and `stop()`, and says
 * how many times each component rendered, by name.
 */

/** The fiber tags of the components that render: function, forward-ref, simple memo and memo. */
const COMPONENT_TAGS = new Set([0, 11, 14, 15]);
const PERFORMED_WORK = 1;

interface Fiber {
  tag: number;
  flags: number;
  type: { displayName?: string; name?: string; type?: { name?: string } } | null;
  child: Fiber | null;
  sibling: Fiber | null;
  alternate: Fiber | null;
}

export interface Renders {
  /** Components that rendered, in all the commits. */
  rendered: number;
  commits: number;
  /** How many times each component rendered, by name. */
  byName: Record<string, number>;
}

/** Adds what rendered in the commit of `root` to `into`. */
function countCommit(root: { current: Fiber }, into: Renders) {
  // An iterative walk: a board is deep, and the stack of a recursive one is not ours to spend.
  const pending: Fiber[] = [root.current];
  while (pending.length > 0) {
    const fiber = pending.pop();
    if (!fiber) continue;
    if (COMPONENT_TAGS.has(fiber.tag) && (fiber.flags & PERFORMED_WORK) !== 0) {
      const type = fiber.type;
      const name = type?.displayName ?? type?.name ?? type?.type?.name ?? "anonymous";
      into.rendered++;
      into.byName[name] = (into.byName[name] ?? 0) + 1;
    }
    if (fiber.sibling) pending.push(fiber.sibling);
    // A subtree React left as it was holds the same fibers as before, with the flags of the commit that drew them.
    const kept = fiber.alternate !== null && fiber.child === fiber.alternate.child;
    if (fiber.child && !kept) pending.push(fiber.child);
  }
}

/** Starts counting; `stop()` ends it and says what was counted. */
export function watchRenders(): { stop: () => Renders } {
  const seen: Renders = { rendered: 0, commits: 0, byName: {} };
  const hooked = globalThis as { __onCommitFiberRoot?: (root: { current: Fiber }) => void };
  hooked.__onCommitFiberRoot = (root) => {
    seen.commits++;
    countCommit(root, seen);
  };
  return {
    stop: () => {
      hooked.__onCommitFiberRoot = undefined;
      return seen;
    },
  };
}
