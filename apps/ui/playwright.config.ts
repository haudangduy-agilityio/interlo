import { defineConfig } from "@playwright/test";

// Uses the Chrome installed on the machine: no browser download.
export default defineConfig({
  testDir: "e2e",
  retries: process.env.CI ? 1 : 0,
  reporter: [["list"]],
  use: {
    baseURL: "http://localhost:5180",
    trace: "on-first-retry",
  },
  projects: [{ name: "chrome", use: { channel: "chrome" } }],
  webServer: {
    command: "npm run dev -- --port 5180 --strictPort",
    url: "http://localhost:5180",
    reuseExistingServer: !process.env.CI,
  },
});
