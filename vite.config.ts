import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

const host = process.env.TAURI_DEV_HOST;
const page = (name: string) => fileURLToPath(new URL(`./${name}.html`, import.meta.url));

export default defineConfig({
  plugins: [svelte()],
  // 保留 Rust 编译输出，不清屏
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    target: "es2022",
    // 每个 Tauri 窗口对应一个独立页面
    rolldownOptions: {
      input: {
        popup: page("popup"),
        settings: page("settings"),
      },
    },
  },
});
