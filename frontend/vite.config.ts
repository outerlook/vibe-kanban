// vite.config.ts
import { sentryVitePlugin } from "@sentry/vite-plugin";
import { defineConfig, Plugin } from "vite";
import react from "@vitejs/plugin-react";
import path from "path";
import fs from "fs";

const manualChunkGroups: Record<string, string[]> = {
  "react-vendor": ["react", "react-dom", "react-router-dom"],
  "ui-vendor": [
    "@radix-ui/react-dropdown-menu",
    "@radix-ui/react-label",
    "@radix-ui/react-select",
    "@radix-ui/react-slot",
    "@radix-ui/react-switch",
    "@radix-ui/react-toggle-group",
    "@radix-ui/react-tooltip",
    "@ebay/nice-modal-react",
    "embla-carousel-react",
    "framer-motion",
    "lucide-react",
    "react-hotkeys-hook",
    "react-resizable-panels",
  ],
  editor: [
    "@uiw/react-codemirror",
    "@codemirror/lang-json",
    "@codemirror/language",
    "@codemirror/lint",
    "@codemirror/view",
  ],
  "lexical-vendor": ["@lexical", "lexical"],
  "diff-react-vendor": ["@git-diff-view/react"],
  "diff-file-vendor": ["@git-diff-view/file"],
  "diff-utils-vendor": ["rfc6902"],
  "diff-core-vendor": ["@git-diff-view/core", "diff", "fast-diff"],
  "highlight-vendor": ["highlight.js"],
  "lowlight-vendor": ["lowlight"],
  "data-vendor": ["@tanstack", "zustand", "wa-sqlite"],
  "i18n-vendor": ["i18next", "react-i18next"],
  "analytics-vendor": ["@sentry", "posthog-js"],
  "form-vendor": ["@rjsf", "ajv", "ajv-formats"],
  "misc-vendor": [
    "@dnd-kit",
    "@virtuoso.dev",
    "react-virtuoso",
    "react-dropzone",
    "simple-icons",
    "vibe-kanban-web-companion",
    "lodash",
    "clsx",
    "class-variance-authority",
    "tailwind-merge",
    "tailwindcss-animate",
    "fancy-ansi",
  ],
};

const matchesChunk = (id: string, packages: string[]) =>
  packages.some((pkg) => id.includes(`/node_modules/${pkg}/`));

function executorSchemasPlugin(): Plugin {
  const VIRTUAL_ID = "virtual:executor-schemas";
  const RESOLVED_VIRTUAL_ID = "\0" + VIRTUAL_ID;

  return {
    name: "executor-schemas-plugin",
    resolveId(id) {
      if (id === VIRTUAL_ID) return RESOLVED_VIRTUAL_ID; // keep it virtual
      return null;
    },
    load(id) {
      if (id !== RESOLVED_VIRTUAL_ID) return null;

      const schemasDir = path.resolve(__dirname, "../shared/schemas");
      const files = fs.existsSync(schemasDir)
        ? fs.readdirSync(schemasDir).filter((f) => f.endsWith(".json"))
        : [];

      const imports: string[] = [];
      const entries: string[] = [];

      files.forEach((file, i) => {
        const varName = `__schema_${i}`;
        const importPath = `shared/schemas/${file}`; // uses your alias
        const key = file.replace(/\.json$/, "").toUpperCase(); // claude_code -> CLAUDE_CODE
        imports.push(`import ${varName} from "${importPath}";`);
        entries.push(`  "${key}": ${varName}`);
      });

      // IMPORTANT: pure JS (no TS types), and quote keys.
      const code = `
${imports.join("\n")}

export const schemas = {
${entries.join(",\n")}
};

export default schemas;
`;
      return code;
    },
  };
}

export default defineConfig({
  plugins: [
    react(),
    sentryVitePlugin({ org: "bloop-ai", project: "vibe-kanban" }),
    executorSchemasPlugin(),
  ],
  resolve: {
    alias: [
      { find: "@", replacement: path.resolve(__dirname, "./src") },
      { find: "shared", replacement: path.resolve(__dirname, "../shared") },
      {
        find: "@git-diff-view/lowlight",
        replacement: path.resolve(
          __dirname,
          "./src/shims/git-diff-view-lowlight.ts"
        ),
      },
      {
        find: /^highlight\.js$/,
        replacement: "highlight.js/lib/common",
      },
    ],
  },
  server: {
    port: parseInt(process.env.FRONTEND_PORT || "3000"),
    proxy: {
      "/api": {
        target: `http://localhost:${process.env.BACKEND_PORT || "3001"}`,
        changeOrigin: true,
        ws: true,
      }
    },
    fs: {
      allow: [path.resolve(__dirname, "."), path.resolve(__dirname, "..")],
    },
    open: process.env.VITE_OPEN === "true",
  },
  optimizeDeps: {
    exclude: ["wa-sqlite"],
  },
  build: {
    sourcemap: "hidden",
    rollupOptions: {
      output: {
        entryFileNames: "assets/app-[hash].js",
        manualChunks(id) {
          if (!id.includes("node_modules")) return undefined;
          for (const [chunkName, packages] of Object.entries(
            manualChunkGroups
          )) {
            if (matchesChunk(id, packages)) return chunkName;
          }
          return undefined;
        },
      },
    },
  },
});
