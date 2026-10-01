import { defineConfig } from "@playwright/test";
export default defineConfig({
  testDir: "./tests",
  testMatch: "**/*.e2e.ts",
  use: {
    baseURL: "http://127.0.0.1:1420",
    viewport: { width: 1440, height: 1040 },
    screenshot: "only-on-failure",
  },
  webServer: {
    command: "npm run dev",
    url: "http://127.0.0.1:1420",
    reuseExistingServer: true,
  },
  reporter: "list",
});
