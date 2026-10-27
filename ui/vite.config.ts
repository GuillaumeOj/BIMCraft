import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react()],
  // The desktop window loads the dev UI from this exact address (crates/desktop-app/tauri.conf.json).
  server: { port: 5173, strictPort: true },
});
