import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { resolve } from "path";
import { rmSync } from "fs";

// Tauri expects assets at fixed paths; clear base, default port 5173.
//
// Multi-page build — EVERY window declared in src-tauri/tauri.conf.json
// must have a matching rollup input here, otherwise the HTML file is
// missing from dist/ and the window shows a WebView2 error page in
// release builds. (Dev mode hides this: the Vite server serves any HTML
// file on demand, so a missing rollup input only breaks production.)
//
//   tauri.conf.json window  ->  rollup input
//   setup     setup.html    ->  setup
//   settings  settings.html ->  settings
//   sidebar   sidebar.html  ->  sidebar  (unified: Assistant + Command Hub
//                            + Architect + PR List views in one window)
//   stage     stage.html    ->  stage   (fullscreen single-stage shell —
//                            hosts the voice orb + loading indicator too,
//                            see stage/OrbFrame.tsx + stage/LoadingIndicator.tsx;
//                            the old `main`/`loading` windows + their
//                            index.html/loading.html entries are retired)
//
// Silero VAD files (model + worklet) live in public/ so Vite serves them
// as-is without any transformation. ONNX WASM runtime is loaded from CDN
// to avoid the known Vite+onnxruntime-web dynamic import incompatibility.
// Design-reference files that live in public/ for dev use but must not be embedded in the release
// executable (nothing in src/ loads them).
const DEV_ONLY_PUBLIC_FILES = ["v2.mp4"];

export default defineConfig({
  plugins: [
    react(),
    {
      name: "drop-dev-only-public-files",
      apply: "build",
      closeBundle() {
        for (const name of DEV_ONLY_PUBLIC_FILES) {
          rmSync(resolve(__dirname, "dist", name), { force: true });
        }
      },
    },
  ],
  base: "./",
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    headers: {
      "Cache-Control": "no-cache, no-store, must-revalidate",
    },
  },
  envPrefix: ["VITE_"],
  build: {
    target: "es2022",
    minify: "esbuild",
    sourcemap: false,
    chunkSizeWarningLimit: 2000,
    rollupOptions: {
      input: {
        setup: resolve(__dirname, "setup.html"),
        settings: resolve(__dirname, "settings.html"),
        sidebar: resolve(__dirname, "sidebar.html"),
        stage: resolve(__dirname, "stage.html"),
        companionHud: resolve(__dirname, "companion-hud.html"),
        orbStudio: resolve(__dirname, "orb-studio.html"),
        hand: resolve(__dirname, "hand.html"),
        // caption-showcase / pebble-showcase / caption-tuner are design playgrounds: the Vite dev
        // server still serves them on demand, but they are not shipped in the release bundle.
        githubDashboard: resolve(__dirname, "github-dashboard.html"),
      },
    },
  },
});
