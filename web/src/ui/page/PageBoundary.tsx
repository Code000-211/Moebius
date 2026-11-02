/**
 * The frame around the page inside the shell, which catches a page that never arrived.
 *
 * Every page is fetched when its route opens (see `app/routes.tsx`), so between the click and the answer the browser
 * holds nothing of it. When that fetch fails — the server stopped answering in the moment between, or a new version
 * replaced the file this tab was told about — React has no element to draw, and without this frame the whole app is
 * exchanged for the router's own screen: a stack trace that says neither what happened nor how to get back, and takes
 * the sidebar, the banner of the connection and its retry with it. The frame keeps the shell standing and says what
 * failed in the words the rest of the app uses.
 *
 * Moving to another page tries again (`resetKey` is the address the shell is at), so one page that failed does not
 * block the others. A reload is the only honest retry for the page that failed: React remembers that the file failed
 * and asks for it again only from a fresh page, so the browser has to start over. An error boundary is a class, the one place the web app has none of.
 */
import { Component, type ErrorInfo, type ReactNode } from "react";
import { Button } from "../components/Button";
import { Notice } from "./Notice";

export class PageBoundary extends Component<
  { children: ReactNode; resetKey?: string },
  { failed: boolean }
> {
  state: { failed: boolean } = { failed: false };

  componentDidUpdate(before: { resetKey?: string }): void {
    if (this.state.failed && before.resetKey !== this.props.resetKey)
      this.setState({ failed: false });
  }

  static getDerivedStateFromError(): { failed: boolean } {
    return { failed: true };
  }

  componentDidCatch(error: unknown, info: ErrorInfo): void {
    // The screen holds the words for the person using the app; the reason belongs to whoever reads the console.
    console.error("the page could not be shown", error, info.componentStack);
  }

  render(): ReactNode {
    if (!this.state.failed) return this.props.children;
    return (
      <Notice
        tone="error"
        action={
          <Button size="sm" variant="secondary" onClick={() => window.location.reload()}>
            Reload
          </Button>
        }
      >
        The page could not be loaded. The server may not be answering, or a newer version replaced
        it.
      </Notice>
    );
  }
}
