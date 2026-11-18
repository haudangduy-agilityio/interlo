import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { AsyncContent, Banner, Button, EmptyState, ErrorState, LoadingState, Toast } from "./index";

describe("shared UI", () => {
  it("LoadingState has role=status", () => {
    const html = renderToStaticMarkup(<LoadingState label="Loading meetings" />);
    expect(html).toContain('role="status"');
    expect(html).toContain("Loading meetings");
  });

  it("Toast has role=status and a labelled dismiss button", () => {
    const html = renderToStaticMarkup(<Toast message="Saved." onDismiss={() => {}} />);
    expect(html).toContain('role="status"');
    expect(html).toContain('aria-label="Dismiss notification"');
  });

  it("ErrorState shows the message and a Retry button only when it can retry", () => {
    const withRetry = renderToStaticMarkup(
      <ErrorState message="Could not load." onRetry={() => {}} />,
    );
    expect(withRetry).toContain("Could not load.");
    expect(withRetry).toContain(">Retry</button>");
    expect(renderToStaticMarkup(<ErrorState message="Could not load." />)).not.toContain("Retry");
  });

  it("Banner uses alert for errors and status for the rest", () => {
    expect(renderToStaticMarkup(<Banner kind="error">x</Banner>)).toContain('role="alert"');
    expect(renderToStaticMarkup(<Banner kind="warning">x</Banner>)).toContain('role="status"');
    expect(renderToStaticMarkup(<Banner kind="info">x</Banner>)).toContain("banner--info");
  });

  it("Button renders a real button that does not submit by default", () => {
    const html = renderToStaticMarkup(<Button>Go</Button>);
    expect(html.startsWith("<button")).toBe(true);
    expect(html).toContain('type="button"');
  });

  it("EmptyState shows title, description and action", () => {
    const html = renderToStaticMarkup(
      <EmptyState
        title="No meetings yet"
        description="Nothing here."
        action={<Button>Add</Button>}
      />,
    );
    expect(html).toContain("No meetings yet");
    expect(html).toContain("Nothing here.");
    expect(html).toContain(">Add</button>");
  });

  it("AsyncContent picks loading, error, empty and done", () => {
    const children = (n: number[]) => <p>count {n.length}</p>;
    const isEmpty = (n: number[]) => n.length === 0;
    const empty = <p>nothing</p>;
    expect(
      renderToStaticMarkup(<AsyncContent state={{ status: "loading" }}>{children}</AsyncContent>),
    ).toContain('role="status"');
    expect(
      renderToStaticMarkup(
        <AsyncContent
          state={{ status: "error", error: { code: "UNKNOWN", message: "bad" } }}
          onRetry={() => {}}
        >
          {children}
        </AsyncContent>,
      ),
    ).toContain("Retry");
    expect(
      renderToStaticMarkup(
        <AsyncContent state={{ status: "done", data: [] }} isEmpty={isEmpty} empty={empty}>
          {children}
        </AsyncContent>,
      ),
    ).toContain("nothing");
    expect(
      renderToStaticMarkup(
        <AsyncContent state={{ status: "done", data: [1, 2] }} isEmpty={isEmpty} empty={empty}>
          {children}
        </AsyncContent>,
      ),
    ).toContain("count 2");
  });
});
