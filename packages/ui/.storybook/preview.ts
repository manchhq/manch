import type { Preview } from "@storybook/react";
import "../src/styles.css";

const preview: Preview = {
  parameters: {
    controls: { matchers: { color: /(background|color)$/i, date: /Date$/i } },
    backgrounds: {
      default: "dark",
    },
  },
  decorators: [
    (Story) => {
      document.documentElement.setAttribute("data-theme", "dark");
      return Story();
    },
  ],
};

export default preview;
