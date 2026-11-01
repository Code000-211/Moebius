import { describe, expect, it } from "vitest";
import {
  afterScroll,
  afterSend,
  afterToggle,
  atEnd,
  byPerson,
  END_SLACK,
  GESTURE_WINDOW_MS,
  shouldRepin,
} from "./follow";

const view = { height: 1000, viewport: 400 };

describe("atEnd", () => {
  it("is true at the end, give or take a line", () => {
    expect(atEnd({ ...view, top: 600 })).toBe(true);
    expect(atEnd({ ...view, top: 600 - END_SLACK })).toBe(true);
    expect(atEnd({ ...view, top: 600 - END_SLACK - 1 })).toBe(false);
  });
});

describe("afterScroll", () => {
  it("stops following when the person scrolls up", () => {
    const state = afterScroll({ follow: true, top: 600 }, { ...view, top: 300 }, true);
    expect(state).toEqual({ follow: false, top: 300 });
  });

  it("keeps following when the list moves itself up, as it does while it measures a taller turn", () => {
    const state = afterScroll(
      { follow: true, top: 1589 },
      { height: 2093, viewport: 504, top: 0 },
      false,
    );
    expect(state).toEqual({ follow: true, top: 0 });
  });

  it("keeps not following while the person is above the end, also when the content grows", () => {
    let state = { follow: false, top: 300 };
    state = afterScroll(state, { ...view, top: 320 }, true);
    expect(state.follow).toBe(false);
    state = afterScroll(state, { height: 2000, viewport: 400, top: 320 }, false);
    expect(state.follow).toBe(false);
  });

  it("follows again when the person reaches the end", () => {
    const state = afterScroll({ follow: false, top: 300 }, { ...view, top: 600 }, true);
    expect(state.follow).toBe(true);
  });

  it("keeps following when the content grows and the view scrolled on", () => {
    const state = afterScroll(
      { follow: true, top: 600 },
      { height: 1500, viewport: 400, top: 1100 },
      false,
    );
    expect(state.follow).toBe(true);
  });

  it("does not stop following when the person only scrolled down", () => {
    const state = afterScroll({ follow: true, top: 100 }, { ...view, top: 200 }, true);
    expect(state.follow).toBe(true);
  });
});

describe("byPerson", () => {
  it("is true shortly after a gesture and false long after it or without one", () => {
    expect(byPerson(1000, 1000 + GESTURE_WINDOW_MS - 1)).toBe(true);
    expect(byPerson(1000, 1000 + GESTURE_WINDOW_MS)).toBe(false);
    expect(byPerson(null, 1000)).toBe(false);
  });
});

describe("shouldRepin", () => {
  it("is true for a following list that is not at its end, and false otherwise", () => {
    expect(shouldRepin({ follow: true, top: 0 }, { ...view, top: 0 })).toBe(true);
    expect(shouldRepin({ follow: true, top: 0 }, { ...view, top: 600 })).toBe(false);
    expect(shouldRepin({ follow: false, top: 0 }, { ...view, top: 0 })).toBe(false);
  });
});

describe("afterToggle", () => {
  it("lets go of the end, so what was opened stays on the screen", () => {
    expect(afterToggle({ follow: true, top: 600 })).toEqual({ follow: false, top: 600 });
    // The end is only taken again by the person, not by the list measuring what it has to draw.
    expect(shouldRepin(afterToggle({ follow: true, top: 600 }), { ...view, top: 600 })).toBe(false);
  });
});

describe("afterSend", () => {
  it("follows again, wherever the person had scrolled", () => {
    expect(afterSend({ follow: false, top: 10 })).toEqual({ follow: true, top: 10 });
  });
});
