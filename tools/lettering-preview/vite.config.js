import { defineConfig } from "vite";
import { fileURLToPath } from "node:url";

// The letters live with the crate that sets labels from them; the dev server may read that folder.
const lettering = fileURLToPath(new URL("../../crates/mercs2_quartermaster/data/lettering", import.meta.url));

export default defineConfig({
  server: {
    open: true,
    fs: { allow: [".", lettering] },
  },
});
