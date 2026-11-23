import * as esbuild from "esbuild";
import { cpSync, mkdirSync, rmSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const dist = join(root, "dist");

export async function build(watch = false) {
  rmSync(dist, { recursive: true, force: true });
  mkdirSync(dist, { recursive: true });
  cpSync(join(root, "index.html"), join(dist, "index.html"));

  const ctx = await esbuild.context({
    entryPoints: [join(root, "src/main.ts")],
    bundle: true,
    format: "esm",
    platform: "browser",
    target: "es2022",
    outdir: dist,
    entryNames: "main",
    minify: !watch,
    sourcemap: watch ? "inline" : false,
    logLevel: "info",
  });

  if (watch) {
    await ctx.watch();
    console.log("esbuild watching...");
  } else {
    await ctx.rebuild();
    await ctx.dispose();
  }
}

const isMain = process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1];
if (isMain) {
  build(process.argv.includes("--watch")).catch((e) => {
    console.error(e);
    process.exit(1);
  });
}
