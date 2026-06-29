import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactNode } from "react";

vi.mock("../lib/api", () => ({
  saveApiKey: vi.fn().mockResolvedValue(undefined),
  listConfiguredProviders: vi.fn(),
}));
import { saveApiKey } from "../lib/api";
import Settings from "./Settings";

function wrap(children: ReactNode) {
  const qc = new QueryClient();
  return <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
}

describe("Settings container", () => {
  beforeEach(() => vi.clearAllMocks());
  it("calls saveApiKey via the mutation", async () => {
    render(wrap(<Settings />));
    await userEvent.type(screen.getByLabelText(/api key/i), "sk-test");
    await userEvent.click(screen.getByRole("button", { name: /save/i }));
    expect(saveApiKey).toHaveBeenCalledWith("anthropic", "sk-test");
  });
});
