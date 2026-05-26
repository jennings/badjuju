import { defineConfig } from "@vscode/test-cli";

export default defineConfig({
  files: "out/test/suite/**/*.test.js",
  extensionDevelopmentPath: ".",
  mocha: {
    ui: "tdd",
    timeout: 30_000,
    color: true,
  },
});
