/**
 * Test helper for anything that draws a `VirtualList` (or a `Virtuoso`) in jsdom.
 *
 * jsdom has no layout: every element is 0px, so a virtual list believes its viewport is empty and draws no rows. The
 * contract of this helper:
 *
 * - `mockViewport({ height, rowHeight })` makes every element report `height` (default 600px) for `offsetHeight`,
 *   `clientHeight` and `getBoundingClientRect`, a width of 800px, and makes `ResizeObserver` report those sizes when
 *   an element is observed, and `rowHeight` (default 36px) for an element that is a row or a group row
 *   (`data-testid` of `list-row`, `group-row`, or a board card or stack, `card-<n>` and `stack-<n>`; a group row is 30px). A list then draws the rows that fit in
 *   `height`, plus the screenful it keeps around them.
 * - `requestAnimationFrame` runs its callback at once, so what a list measures on the next frame is measured now.
 * - It returns a function that puts everything back; call it in `afterEach`.
 * - Every element has an offset parent (a virtual list ignores one without), a scroller has room to scroll, and
 *   `scrollTo` sets `scrollTop` and fires `scroll`.
 * - Scrolling by a wheel is not simulated: a test that needs the end of a long list moves the selection there (End key) or calls
 *   the list's `scrollToKey`, which asks the list to scroll to an item, and finds that the item is drawn.
 *
 * ```ts
 * let restore: () => void;
 * beforeEach(() => { restore = mockViewport({ height: 600 }); });
 * afterEach(() => restore());
 * ```
 */

interface Sizes {
  height?: number;
  rowHeight?: number;
  width?: number;
}

const GROUP_HEIGHT = 30;
/** A card of a board column, or the stack with its sub-issues: a row as well. */
const CARD = /^(?:card|stack)-\d+$/;

export function mockViewport({
  height = 600,
  rowHeight = 36,
  width = 800,
}: Sizes = {}): () => void {
  const proto = HTMLElement.prototype;
  const own = (name: string) => Object.getOwnPropertyDescriptor(proto, name);
  const saved = {
    offsetHeight: own("offsetHeight"),
    clientHeight: own("clientHeight"),
    offsetWidth: own("offsetWidth"),
    clientWidth: own("clientWidth"),
    offsetParent: own("offsetParent"),
    scrollHeight: Object.getOwnPropertyDescriptor(Element.prototype, "scrollHeight"),
    scrollTo: Element.prototype.scrollTo,
    getBoundingClientRect: proto.getBoundingClientRect,
    ResizeObserver: globalThis.ResizeObserver,
    requestAnimationFrame: globalThis.requestAnimationFrame,
  };
  // A list measures what it draws on the next frame; here the frame is now, so a test need not wait for one.
  globalThis.requestAnimationFrame = (callback: FrameRequestCallback) => {
    callback(0);
    return 0;
  };

  const heightOf = (el: Element) => {
    const id = el.getAttribute("data-testid");
    if (id === "list-row" || (id && CARD.test(id))) return rowHeight;
    if (id === "group-row") return GROUP_HEIGHT;
    // The wrappers Virtuoso and a caller put round an item are as tall as the item in them.
    let inner: Element | null = el;
    while (inner && inner.children.length === 1) {
      inner = inner.children[0] ?? null;
      const innerId = inner?.getAttribute("data-testid");
      if (innerId === "list-row" || (innerId && CARD.test(innerId))) return rowHeight;
      if (innerId === "group-row") return GROUP_HEIGHT;
    }
    return height;
  };

  Object.defineProperty(proto, "offsetHeight", {
    configurable: true,
    get(this: HTMLElement) {
      return heightOf(this);
    },
  });
  Object.defineProperty(proto, "clientHeight", {
    configurable: true,
    get(this: HTMLElement) {
      return heightOf(this);
    },
  });
  // A virtual list ignores an element with no offset parent, which in jsdom is every one.
  Object.defineProperty(proto, "offsetParent", {
    configurable: true,
    get(this: HTMLElement) {
      return this.parentElement;
    },
  });
  // Room to scroll: a scroller is taller inside than out, so what it is scrolled to can be told from the top.
  Object.defineProperty(Element.prototype, "scrollHeight", {
    configurable: true,
    get: () => 10_000_000,
  });
  Element.prototype.scrollTo = function (this: Element, ...args: unknown[]) {
    const top =
      typeof args[0] === "object" ? (args[0] as ScrollToOptions).top : (args[1] as number);
    if (typeof top === "number") {
      this.scrollTop = top;
      this.dispatchEvent(new Event("scroll"));
    }
  } as typeof Element.prototype.scrollTo;
  Object.defineProperty(proto, "offsetWidth", { configurable: true, get: () => width });
  Object.defineProperty(proto, "clientWidth", { configurable: true, get: () => width });
  proto.getBoundingClientRect = function (this: HTMLElement) {
    const h = heightOf(this);
    return { x: 0, y: 0, top: 0, left: 0, right: width, bottom: h, width, height: h, toJSON() {} };
  };
  globalThis.ResizeObserver = class {
    constructor(private readonly callback: ResizeObserverCallback) {}
    observe(target: Element) {
      const h = heightOf(target);
      const box = [{ inlineSize: width, blockSize: h }];
      this.callback(
        [
          {
            target,
            contentRect: { x: 0, y: 0, top: 0, left: 0, right: width, bottom: h, width, height: h },
            borderBoxSize: box,
            contentBoxSize: box,
            devicePixelContentBoxSize: box,
          } as unknown as ResizeObserverEntry,
        ],
        this,
      );
    }
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver;

  return () => {
    for (const name of ["offsetHeight", "clientHeight", "offsetWidth", "clientWidth"] as const) {
      const descriptor = saved[name];
      if (descriptor) Object.defineProperty(proto, name, descriptor);
      else delete (proto as unknown as Record<string, unknown>)[name];
    }
    for (const [name, target] of [
      ["offsetParent", proto],
      ["scrollHeight", Element.prototype],
    ] as const) {
      const descriptor = saved[name];
      if (descriptor) Object.defineProperty(target, name, descriptor);
      else delete (target as unknown as Record<string, unknown>)[name];
    }
    Element.prototype.scrollTo = saved.scrollTo;
    proto.getBoundingClientRect = saved.getBoundingClientRect;
    globalThis.ResizeObserver = saved.ResizeObserver;
    globalThis.requestAnimationFrame = saved.requestAnimationFrame;
  };
}
