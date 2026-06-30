import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { SearchBar } from "./SearchBar";
describe("SearchBar", () => {
  it("changes and submits", () => {
    const onChange = vi.fn(); const onSubmit = vi.fn();
    render(<SearchBar value="" onChange={onChange} onSubmit={onSubmit} />);
    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "precedent" } });
    expect(onChange).toHaveBeenCalledWith("precedent");
    fireEvent.submit(screen.getByRole("search"));
    expect(onSubmit).toHaveBeenCalledOnce();
  });
});
