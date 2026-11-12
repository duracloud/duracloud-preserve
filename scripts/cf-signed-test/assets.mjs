// Generate synthetic HLS families shaped like client content into .work/assets/<final key layout>
// (restricted/<space>/<prefix>/… and open-hls/<prefix>/…). Requires ffmpeg.
// Usage: node assets.mjs [--duration 120]
import { mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { parseArgs } from "node:util";
import { ASSETS, FAMILIES, fail, familyDir, run } from "./lib.mjs";

const { values } = parseArgs({ options: { duration: { type: "string", default: "120" } } });
const t = values.duration;

const video = ["-f", "lavfi", "-i", "testsrc2=size=640x360:rate=25"];
const tone = ["-f", "lavfi", "-i", "sine=frequency=440"];
const vod = ["-hls_playlist_type", "vod"];
const x264 = ["-c:v", "libx264", "-preset", "veryfast", "-g", "50"];

// Single variant, one .ts per variant addressed with EXT-X-BYTERANGE
const singleFile = (prefix) => [
  ...video,
  ...tone,
  "-t",
  t,
  ...x264,
  "-b:v",
  "400k",
  "-c:a",
  "aac",
  "-b:a",
  "160k",
  "-hls_time",
  "6",
  ...vod,
  "-hls_flags",
  "single_file",
  "-master_pl_name",
  `${prefix}-playlist.m3u8`,
  "-hls_segment_filename",
  `${prefix}-a160k_v4.ts`,
  `${prefix}-a160k_v4.m3u8`,
];

// Two variants, one file per segment
const segmented = (prefix) => [
  ...video,
  ...tone,
  "-t",
  t,
  "-filter_complex",
  "[0:v]split=2[v1][v2];[v2]scale=320:180[v2s]",
  "-map",
  "[v1]",
  "-map",
  "1:a",
  "-map",
  "[v2s]",
  "-map",
  "1:a",
  ...x264,
  "-b:v:0",
  "400k",
  "-b:v:1",
  "150k",
  "-c:a",
  "aac",
  "-b:a:0",
  "160k",
  "-b:a:1",
  "96k",
  "-hls_time",
  "6",
  ...vod,
  "-start_number",
  "1",
  "-var_stream_map",
  "v:0,a:0,name:a160k_v4 v:1,a:1,name:a96k_v4",
  "-master_pl_name",
  `${prefix}-playlist.m3u8`,
  "-hls_segment_filename",
  `${prefix}-%v-%05d.ts`,
  `${prefix}-%v.m3u8`,
];

// Audio only, shaped like the real closed-hls sample (7s byte-range segments)
const audioOnly = (prefix) => [
  ...tone,
  "-t",
  t,
  "-vn",
  "-c:a",
  "aac",
  "-b:a",
  "160k",
  "-hls_time",
  "7",
  ...vod,
  "-hls_flags",
  "single_file",
  "-master_pl_name",
  `${prefix}-playlist.m3u8`,
  "-hls_segment_filename",
  `${prefix}-a160k_v4.ts`,
  `${prefix}-a160k_v4.m3u8`,
];

const shapes = { TEST_C_0002: segmented, TEST_X_0002: audioOnly };

rmSync(ASSETS, { recursive: true, force: true });

for (const { space, prefix } of FAMILIES) {
  const dir = familyDir(space, prefix);
  mkdirSync(dir, { recursive: true });
  console.log(`Generating ${space}/${prefix}`);
  run("ffmpeg", ["-loglevel", "error", "-y", ...(shapes[prefix] ?? singleFile)(prefix)], {
    cwd: dir,
  });

  // Every reference must be a bare filename, as in migrated client playlists
  for (const file of readdirSync(dir).filter((f) => f.endsWith(".m3u8"))) {
    for (const line of readFileSync(join(dir, file), "utf8").split(/\r?\n/)) {
      if (line && !line.startsWith("#") && !/^[A-Za-z0-9_.-]+$/.test(line)) {
        fail(`${space}/${prefix}/${file} references ${line}, not a bare filename`);
      }
    }
  }
}

// Unsigned requests for this must be denied before the rest of restricted/ is uploaded
writeFileSync(join(ASSETS, "restricted", "canary.txt"), "signed delivery canary\n");

console.log(`Assets written to ${ASSETS}`);
