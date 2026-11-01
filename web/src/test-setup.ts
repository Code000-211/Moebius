/** Test setup: cleans the DOM that a component test rendered. */
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

afterEach(() => cleanup());
