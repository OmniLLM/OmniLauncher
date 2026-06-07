/**
 * ResultList component tests.
 *
 * Why these specific cases:
 *  - groupTitle: a real bug caught manually in this exact spot — the prop was
 *    destructured but never rendered, so "★ Favorites" silently disappeared.
 *    This test would have caught it instantly. Keep it as a regression guard.
 *  - Keyboard navigation (↑/↓, Enter, Cmd+1-9): the launcher is keyboard-first;
 *    breaking these is a silent UX regression that's easy to ship without tests.
 *  - Favorites toggle: exercises the optional onToggleFavorite path and the
 *    favorites Set prop.
 *  - Empty-result state: previously had a dead branch in ResultList that the
 *    9ff6d1d commit removed. Lock the current contract: empty list renders
 *    nothing list-like (App gates on length > 0).
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import ResultList from "./ResultList";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

type QueryResult = {
  id: string;
  title: string;
  subtitle?: string;
  icon?: string;
  score: number;
  action_type: string;
  action_data: string;
  source?: string;
};

const make = (over: Partial<QueryResult> & { id: string; title: string }): QueryResult => ({
  subtitle: "",
  icon: "📄",
  score: 1,
  action_type: "open",
  action_data: "",
  source: "files",
  ...over,
});

const FILE_RESULTS: QueryResult[] = [
  make({ id: "a", title: "alpha.md", source: "files" }),
  make({ id: "b", title: "beta.md", source: "files" }),
  make({ id: "c", title: "gamma.md", source: "files" }),
];

const MIXED_RESULTS: QueryResult[] = [
  make({ id: "f1", title: "notes.md", source: "files" }),
  make({ id: "f2", title: "todo.md", source: "files" }),
  make({ id: "c1", title: "Calculator: 2+2 = 4", source: "calculator" }),
];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/**
 * The component listens on `window` for keydown (so it works while the input
 * has focus). userEvent.keyboard targets document.activeElement, which works
 * for inputs but not for our case — we dispatch directly on window instead.
 */
function pressKey(key: string, init: Partial<KeyboardEventInit> = {}) {
  fireEvent.keyDown(window, { key, ...init });
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

describe("ResultList — rendering", () => {
  it("renders each result row with its title", () => {
    render(
      <ResultList results={FILE_RESULTS} query="" onExecute={() => {}} />,
    );
    // Scope to the listbox: the footer actionbar also renders the selected
    // item's title, so a bare getByText would match twice.
    const listbox = screen.getByRole("listbox");
    expect(within(listbox).getByText("alpha.md")).toBeInTheDocument();
    expect(within(listbox).getByText("beta.md")).toBeInTheDocument();
    expect(within(listbox).getByText("gamma.md")).toBeInTheDocument();
  });

  it("uses the listbox + option ARIA roles", () => {
    render(
      <ResultList results={FILE_RESULTS} query="" onExecute={() => {}} />,
    );
    const listbox = screen.getByRole("listbox");
    expect(listbox).toBeInTheDocument();
    const options = within(listbox).getAllByRole("option");
    expect(options).toHaveLength(3);
  });

  it("marks the first row aria-selected on initial render", () => {
    render(
      <ResultList results={FILE_RESULTS} query="" onExecute={() => {}} />,
    );
    const options = screen.getAllByRole("option");
    expect(options[0]).toHaveAttribute("aria-selected", "true");
    expect(options[1]).toHaveAttribute("aria-selected", "false");
  });
});

// ---------------------------------------------------------------------------
// groupTitle — regression for the silently-dropped prop bug
// ---------------------------------------------------------------------------

describe("ResultList — groupTitle (regression)", () => {
  it("renders the explicit groupTitle header when provided", () => {
    render(
      <ResultList
        results={FILE_RESULTS}
        query=""
        onExecute={() => {}}
        groupTitle="★ Favorites"
      />,
    );
    expect(screen.getByText("★ Favorites")).toBeInTheDocument();
  });

  it("suppresses per-source auto-headers when groupTitle is set", () => {
    // MIXED_RESULTS has two sources ("files", "calculator") — normally that
    // would trigger per-source headers. An explicit groupTitle should win
    // and the per-source headers should NOT render.
    render(
      <ResultList
        results={MIXED_RESULTS}
        query=""
        onExecute={() => {}}
        groupTitle="★ Favorites"
      />,
    );
    expect(screen.getByText("★ Favorites")).toBeInTheDocument();
    expect(screen.queryByText("Files")).not.toBeInTheDocument();
    expect(screen.queryByText("Calculator")).not.toBeInTheDocument();
  });

  it("shows per-source headers when groupTitle is absent and >1 group", () => {
    render(
      <ResultList results={MIXED_RESULTS} query="" onExecute={() => {}} />,
    );
    // prettifySource("files") → "Files", prettifySource("calculator") → "Calculator"
    expect(screen.getByText("Files")).toBeInTheDocument();
    expect(screen.getByText("Calculator")).toBeInTheDocument();
  });

  it("omits the header when there's only one group and no groupTitle", () => {
    render(
      <ResultList results={FILE_RESULTS} query="" onExecute={() => {}} />,
    );
    expect(screen.queryByText("Files")).not.toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// Keyboard navigation — the launcher is keyboard-first
// ---------------------------------------------------------------------------

describe("ResultList — keyboard navigation", () => {
  it("ArrowDown moves selection down by one", () => {
    render(
      <ResultList results={FILE_RESULTS} query="" onExecute={() => {}} />,
    );
    pressKey("ArrowDown");
    const options = screen.getAllByRole("option");
    expect(options[1]).toHaveAttribute("aria-selected", "true");
    expect(options[0]).toHaveAttribute("aria-selected", "false");
  });

  it("ArrowUp moves selection up by one", () => {
    render(
      <ResultList results={FILE_RESULTS} query="" onExecute={() => {}} />,
    );
    pressKey("ArrowDown");
    pressKey("ArrowDown");
    pressKey("ArrowUp");
    expect(screen.getAllByRole("option")[1]).toHaveAttribute(
      "aria-selected",
      "true",
    );
  });

  it("ArrowDown clamps at the last result", () => {
    render(
      <ResultList results={FILE_RESULTS} query="" onExecute={() => {}} />,
    );
    pressKey("ArrowDown");
    pressKey("ArrowDown");
    pressKey("ArrowDown"); // past end
    pressKey("ArrowDown");
    expect(screen.getAllByRole("option")[2]).toHaveAttribute(
      "aria-selected",
      "true",
    );
  });

  it("ArrowUp clamps at zero", () => {
    render(
      <ResultList results={FILE_RESULTS} query="" onExecute={() => {}} />,
    );
    pressKey("ArrowUp");
    pressKey("ArrowUp");
    expect(screen.getAllByRole("option")[0]).toHaveAttribute(
      "aria-selected",
      "true",
    );
  });

  it("Home jumps to first, End jumps to last", () => {
    render(
      <ResultList results={FILE_RESULTS} query="" onExecute={() => {}} />,
    );
    pressKey("End");
    expect(screen.getAllByRole("option")[2]).toHaveAttribute(
      "aria-selected",
      "true",
    );
    pressKey("Home");
    expect(screen.getAllByRole("option")[0]).toHaveAttribute(
      "aria-selected",
      "true",
    );
  });

  it("Enter executes the currently selected result", () => {
    const onExecute = vi.fn();
    render(
      <ResultList results={FILE_RESULTS} query="" onExecute={onExecute} />,
    );
    pressKey("ArrowDown");
    pressKey("Enter");
    expect(onExecute).toHaveBeenCalledTimes(1);
    expect(onExecute).toHaveBeenCalledWith(FILE_RESULTS[1]);
  });

  it("Ctrl+1 executes the 1st result on non-mac (jsdom default platform)", () => {
    // jsdom's navigator.platform is "" or "Linux x86_64" — IS_MAC is false,
    // so the Ctrl modifier is the active one.
    const onExecute = vi.fn();
    render(
      <ResultList results={FILE_RESULTS} query="" onExecute={onExecute} />,
    );
    pressKey("2", { ctrlKey: true });
    expect(onExecute).toHaveBeenCalledTimes(1);
    expect(onExecute).toHaveBeenCalledWith(FILE_RESULTS[1]);
  });

  it("Ctrl+N is a no-op when there's no Nth result", () => {
    const onExecute = vi.fn();
    render(
      <ResultList
        results={FILE_RESULTS.slice(0, 2)}
        query=""
        onExecute={onExecute}
      />,
    );
    pressKey("5", { ctrlKey: true });
    expect(onExecute).not.toHaveBeenCalled();
  });
});

// ---------------------------------------------------------------------------
// Mouse interaction
// ---------------------------------------------------------------------------

describe("ResultList — mouse", () => {
  it("clicking a row executes its result", async () => {
    const onExecute = vi.fn();
    const user = userEvent.setup();
    render(
      <ResultList results={FILE_RESULTS} query="" onExecute={onExecute} />,
    );
    await user.click(screen.getByText("gamma.md"));
    expect(onExecute).toHaveBeenCalledWith(FILE_RESULTS[2]);
  });
});

// ---------------------------------------------------------------------------
// Favorites
// ---------------------------------------------------------------------------

describe("ResultList — favorites", () => {
  it("renders a filled star (★) for favorited items", () => {
    render(
      <ResultList
        results={FILE_RESULTS}
        query=""
        onExecute={() => {}}
        favorites={new Set(["b"])}
        onToggleFavorite={() => {}}
      />,
    );
    const stars = screen.getAllByRole("button", { name: /favorite/i });
    expect(stars[0]).toHaveTextContent("☆");
    expect(stars[1]).toHaveTextContent("★");
    expect(stars[2]).toHaveTextContent("☆");
  });

  it("clicking the star calls onToggleFavorite with the result", async () => {
    const onToggleFavorite = vi.fn();
    const user = userEvent.setup();
    render(
      <ResultList
        results={FILE_RESULTS}
        query=""
        onExecute={() => {}}
        favorites={new Set()}
        onToggleFavorite={onToggleFavorite}
      />,
    );
    const stars = screen.getAllByRole("button", { name: /favorite/i });
    await user.click(stars[1]);
    expect(onToggleFavorite).toHaveBeenCalledWith(FILE_RESULTS[1]);
  });

  it("clicking the star does NOT also trigger onExecute (stopPropagation)", async () => {
    const onExecute = vi.fn();
    const onToggleFavorite = vi.fn();
    const user = userEvent.setup();
    render(
      <ResultList
        results={FILE_RESULTS}
        query=""
        onExecute={onExecute}
        favorites={new Set()}
        onToggleFavorite={onToggleFavorite}
      />,
    );
    const stars = screen.getAllByRole("button", { name: /favorite/i });
    await user.click(stars[0]);
    expect(onToggleFavorite).toHaveBeenCalledTimes(1);
    expect(onExecute).not.toHaveBeenCalled();
  });
});

// ---------------------------------------------------------------------------
// Selection resets when result set changes
// ---------------------------------------------------------------------------

describe("ResultList — selection lifecycle", () => {
  it("resets selection to 0 when the result set changes", () => {
    const { rerender } = render(
      <ResultList results={FILE_RESULTS} query="" onExecute={() => {}} />,
    );
    pressKey("ArrowDown");
    pressKey("ArrowDown");
    expect(screen.getAllByRole("option")[2]).toHaveAttribute(
      "aria-selected",
      "true",
    );
    // New result set arrives
    const NEW = [
      make({ id: "x", title: "x.md" }),
      make({ id: "y", title: "y.md" }),
    ];
    rerender(<ResultList results={NEW} query="" onExecute={() => {}} />);
    expect(screen.getAllByRole("option")[0]).toHaveAttribute(
      "aria-selected",
      "true",
    );
  });
});
