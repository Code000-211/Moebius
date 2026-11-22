/**
 * Where the view of the graph opens, and what it refits to. A graph is fitted into the pane only as far as its step
 * boxes stay readable: below `READABLE_CARD_PX` wide on the screen a box is not fitted to but panned across, so a long
 * graph opens at its start with the rest a scroll away rather than as a row of boxes too small to read.
 */

/** The width of a step box, in graph units. */
export const CARD_WIDTH = 224;
/** A step box is never fitted smaller than this many pixels wide. */
export const READABLE_CARD_PX = 200;
/** The least zoom a fit may use. */
export const READABLE_ZOOM = READABLE_CARD_PX / CARD_WIDTH;

interface Box {
  x: number;
  y: number;
  width: number;
  height: number;
}

/**
 * The view for a graph whose boxes fill `bounds`, in a pane of `pane` pixels. The whole graph is fitted with `padding`
 * of room round it, never zoomed in past 1, and never zoomed out past `READABLE_ZOOM`. When the floor wins the graph is
 * wider or taller than the pane: it is placed with its left edge, and its top edge when it is too tall, at the padding,
 * and the person pans to the rest. When the whole graph fits it is centred.
 */
export function readableViewport(
  bounds: Box,
  pane: { width: number; height: number },
  padding = 32,
): { x: number; y: number; zoom: number } {
  const room = {
    width: Math.max(pane.width - padding * 2, 1),
    height: Math.max(pane.height - padding * 2, 1),
  };
  const fit = Math.min(room.width / bounds.width, room.height / bounds.height, 1);
  const zoom = Math.max(fit, READABLE_ZOOM);
  const place = (extent: number, size: number, origin: number) => {
    const shown = size * zoom;
    return shown <= extent ? (extent - shown) / 2 - origin * zoom : padding - origin * zoom;
  };
  return {
    x: place(pane.width, bounds.width, bounds.x),
    y: place(pane.height, bounds.height, bounds.y),
    zoom,
  };
}
