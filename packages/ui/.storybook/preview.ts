import type { Preview } from "@storybook/react";
import "../src/styles.css";

const preview: Preview = {
  parameters: {
    controls: { matchers: { color: /(background|color)$/i, date: /Date$/i } },
    backgrounds: {
      default: "stage",
      values: [{ name: "stage", value: "#1a1320" }],
    },
  },
  decorators: [
    (Story) => {
      document.documentElement.setAttribute("data-theme", "manch-stage");
      return Story();
    },
  ],
};

export default preview;
