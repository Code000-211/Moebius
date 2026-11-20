/**
 * Lets a test count what React renders. React reads the DevTools hook once, when `react-dom` is loaded, and the setup file
 * that cleans the DOM loads it first, so the hook stands here, before that file. It does nothing until a test sets
 * `__onCommitFiberRoot`, and then it tells the test of every commit with the root that was committed.
 */
(globalThis as Record<string, unknown>).__REACT_DEVTOOLS_GLOBAL_HOOK__ = {
  supportsFiber: true,
  renderers: new Map(),
  inject: () => 1,
  checkDCE: () => {},
  onScheduleFiberRoot: () => {},
  onCommitFiberUnmount: () => {},
  onPostCommitFiberRoot: () => {},
  onCommitFiberRoot: (_id: number, root: unknown) => {
    (globalThis as { __onCommitFiberRoot?: (root: unknown) => void }).__onCommitFiberRoot?.(root);
  },
};
