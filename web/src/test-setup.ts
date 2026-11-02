/**
 * Test setup: cleans the DOM that a component test rendered, and gives the waiting helpers long enough to be worth
 * trusting. A component test renders from a stubbed `fetch`, so `findBy…` waits for a query to land; the default wait
 * is a second, and a whole-suite run builds one jsdom per file, so a test can lose that second to the queue and fail
 * in the middle of three hundred others while passing on its own. Waiting longer costs nothing when the data arrives —
 * the helper returns as soon as it can — and a test that only passes alone is a test nobody can trust.
 */
import { cleanup, configure } from "@testing-library/react";
import { afterEach } from "vitest";

configure({ asyncUtilTimeout: 5_000 });

afterEach(() => cleanup());
