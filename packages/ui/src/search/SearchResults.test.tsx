import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { SearchResults } from "./SearchResults";
describe("SearchResults", () => {
  it("renders hits and opens one", () => {
    const onOpen = vi.fn();
    render(<SearchResults results={[{ kind: "team", id: "tm_1", title: "Discovery", snippet: "find precedent" }]} onOpen={onOpen} />);
    fireEvent.click(screen.getByRole("button", { name: /Discovery/ }));
    expect(onOpen).toHaveBeenCalledWith("team", "tm_1");
  });
});
