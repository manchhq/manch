import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, it, expect, vi } from "vitest";
import { Composer } from "./Composer";

describe("Composer", () => {
  it("calls onChange as the user types", async () => {
    const onChange = vi.fn();
    render(<Composer value="" onChange={onChange} onSend={() => {}} />);
    await userEvent.type(screen.getByRole("textbox"), "h");
    expect(onChange).toHaveBeenCalledWith("h");
  });

  it("disables send when blank and enables with text", () => {
    const { rerender } = render(<Composer value="" onChange={() => {}} onSend={() => {}} />);
    expect((screen.getByRole("button", { name: /send/i }) as HTMLButtonElement).disabled).toBe(true);
    rerender(<Composer value="hi" onChange={() => {}} onSend={() => {}} />);
    expect((screen.getByRole("button", { name: /send/i }) as HTMLButtonElement).disabled).toBe(false);
  });

  it("fires onSend on button click", async () => {
    const onSend = vi.fn();
    render(<Composer value="hi" onChange={() => {}} onSend={onSend} />);
    await userEvent.click(screen.getByRole("button", { name: /send/i }));
    expect(onSend).toHaveBeenCalledOnce();
  });

  it("disables everything while busy", () => {
    render(<Composer value="hi" onChange={() => {}} onSend={() => {}} busy />);
    expect((screen.getByRole("button", { name: /send/i }) as HTMLButtonElement).disabled).toBe(true);
  });
});
