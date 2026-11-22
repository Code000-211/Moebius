/**
 * How a one-row page header gives way when it does not fit, as plain functions the header and its tests share.
 * The header tries levels in order until its content stops extending past its right padding: level 0 is as drawn,
 * level 1 drops the primary button's label (the icon stays), and each level after that moves one more tab, from the
 * right, into the "More" menu. The last tab left is never moved, so the row always shows where it is.
 */

/** What a level of collapsing means for a header with `tabCount` tabs. */
export function partsOfLevel(
  level: number,
  tabCount: number,
): { compactPrimary: boolean; hiddenTabs: number } {
  const hiddenTabs = Math.min(Math.max(level - 1, 0), Math.max(tabCount - 1, 0));
  return { compactPrimary: level >= 1, hiddenTabs };
}

/** The deepest level worth trying: the label gone, then every tab but the first moved into "More". */
export function deepestLevel(tabCount: number): number {
  return 1 + Math.max(tabCount - 1, 0);
}

/** True when content ending at `contentRight` reaches into the header's right padding, ending at `limit`. */
export function overflows(contentRight: number, limit: number): boolean {
  // Sub-pixel layout can leave a fraction over.
  return contentRight > limit + 0.5;
}
