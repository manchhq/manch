import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { VersionBadge } from "./VersionBadge";

describe("VersionBadge", () => {
  it("shows the version when provided", () => {
    render(<VersionBadge version="1.2.3" />);
    expect(screen.getByText("v1.2.3")).toBeInTheDocument();
  });

  it("shows a loading state when version is undefined", () => {
    render(<VersionBadge version={undefined} loading />);
    expect(screen.getByText("loading…")).toBeInTheDocument();
  });

  it("shows an error message when error is set", () => {
    render(<VersionBadge version={undefined} error="boom" />);
    expect(screen.getByText("boom")).toBeInTheDocument();
  });
});
