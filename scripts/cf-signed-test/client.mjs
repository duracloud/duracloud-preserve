// Minimal fake client (harness section 6): signs one directory policy per
// playback session, serves rewritten playlists, and hosts an hls.js player page.
// Not a reference implementation for clients.
// Usage: node client.mjs [--port 8080] [--key test_1] [--stack dcp-test] [--outputs path]
// Run with node directly rather than through npm, so Ctrl+C reaches it cleanly.
import { randomUUID } from "node:crypto";
import { createServer } from "node:http";
import { parseArgs } from "node:util";
import { CONFIG_OPTIONS, FAMILIES, fetchRaw, loadConfig, signer, signParams } from "./lib.mjs";

const { values } = parseArgs({
  options: { ...CONFIG_OPTIONS, port: { type: "string", default: "8080" } },
});
const config = loadConfig(values);
const key = signer(config);
const sessions = new Map();

const BARE = /^[A-Za-z0-9_.-]+$/;

// Playlists come back to this server; everything else goes straight to
// CloudFront with the session's parameters. Only bare filenames are allowed,
// so references can't leave the signed directory.
function rewrite(text, sid, session) {
  const ref = (uri) => {
    if (!BARE.test(uri)) throw new Error(`not a bare filename: ${uri}`);
    return uri.endsWith(".m3u8") ? `/s/${sid}/${uri}` : `${session.dir}${uri}?${session.params}`;
  };
  return text
    .split(/\r?\n/)
    .map((line) => {
      if (line.startsWith("#")) return line.replace(/URI="([^"]+)"/g, (_, u) => `URI="${ref(u)}"`);
      return line.trim() ? ref(line.trim()) : line;
    })
    .join("\n");
}

function send(res, status, body, type = "text/plain; charset=utf-8", headers = {}) {
  res.writeHead(status, { "Content-Type": type, "Cache-Control": "private, no-store", ...headers });
  res.end(body);
}

async function readJson(req) {
  const chunks = [];
  for await (const chunk of req) chunks.push(chunk);
  return JSON.parse(Buffer.concat(chunks).toString() || "{}");
}

async function createSession(req, res) {
  const { space, prefix, ttl } = await readJson(req);
  if (!FAMILIES.some((f) => f.space === space && f.prefix === prefix)) {
    return send(res, 400, "unknown resource");
  }
  if (space === "open-hls") {
    return send(
      res,
      200,
      JSON.stringify({ url: `${config.origin}/open-hls/${prefix}/${prefix}-playlist.m3u8` }),
      "application/json",
    );
  }

  const seconds = Math.min(Math.max(Number(ttl) || 600, 5), 4 * 3600);
  const dir = `${config.baseUrl}${space}/${prefix}/`;
  const sid = randomUUID();
  const exp = Date.now() + seconds * 1000;
  sessions.set(sid, {
    dir,
    exp,
    params: signParams({ resource: `${dir}*`, ttl: seconds, ...key }),
  });
  console.log(`session ${sid.slice(0, 8)} ${space}/${prefix} ttl ${seconds}s`);
  send(
    res,
    200,
    JSON.stringify({ url: `/s/${sid}/${prefix}-playlist.m3u8`, expires: exp }),
    "application/json",
  );
}

async function playlist(res, sid, file) {
  const session = sessions.get(sid);
  if (!session) return send(res, 404, "unknown session");
  if (!BARE.test(file) || !file.endsWith(".m3u8")) return send(res, 400, "bad playlist name");

  const upstream = await fetchRaw(`${session.dir}${file}?${session.params}`);
  if (upstream.status !== 200) {
    console.log(`session ${sid.slice(0, 8)} ${file}: CloudFront ${upstream.status}`);
    return send(res, upstream.status, `CloudFront returned ${upstream.status}`);
  }
  console.log(`session ${sid.slice(0, 8)} ${file}: CloudFront 200`);
  try {
    send(
      res,
      200,
      rewrite(upstream.body.toString(), sid, session),
      "application/vnd.apple.mpegurl",
    );
  } catch (error) {
    send(res, 502, error.message);
  }
}

const page = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Signed delivery test client</title>
<style>
  body { font: 15px system-ui, sans-serif; margin: 2rem; max-width: 720px; }
  video { width: 100%; background: #000; margin: 1rem 0; cursor: pointer; }
  #status, #log { font-family: monospace; white-space: pre-wrap; }
</style>
</head>
<body>
<h1>Signed delivery test client</h1>
<p>${config.baseUrl} (key ${config.key}), page origin <span id="origin"></span></p>
<label>Resource <select id="resource">${FAMILIES.map(
  (f) => `<option value="${f.space}/${f.prefix}">${f.space}/${f.prefix}</option>`,
).join("")}</select></label>
<label>TTL (s) <input id="ttl" type="number" value="600" min="5" style="width:6em"></label>
<label><input id="native" type="checkbox"> Native HLS (Safari)</label>
<button id="play">New session and play</button>
<video id="video" playsinline title="Click to start a session"></video>
<div id="status"></div>
<pre id="log"></pre>
<script src="https://cdn.jsdelivr.net/npm/hls.js@1.5.20/dist/hls.min.js"></script>
<script>
  const video = document.getElementById("video");
  const status = document.getElementById("status");
  document.getElementById("origin").textContent = location.origin;
  const log = document.getElementById("log");
  let hls, expires, timer;

  // Every step goes to the page and the console, so a stall shows where it stopped
  function note(message) {
    console.log("[client] " + message);
    log.textContent += new Date().toLocaleTimeString() + "  " + message + "\\n";
  }

  function fail(message) {
    clearInterval(timer);
    status.textContent = "";
    note("ERROR " + message + " (start a new session to continue)");
  }

  async function play() {
    try {
      log.textContent = "";
      if (hls) { hls.destroy(); hls = null; }
      video.removeAttribute("src");
      video.load();
      const [space, prefix] = document.getElementById("resource").value.split("/");
      note("Creating session for " + space + "/" + prefix);
      const res = await fetch("/sessions", {
        method: "POST",
        body: JSON.stringify({ space, prefix, ttl: document.getElementById("ttl").value }),
      });
      if (!res.ok) return fail("Session failed: " + res.status + " " + (await res.text()));
      const session = await res.json();
      note("Playlist " + session.url);
      video.controls = true;
      expires = session.expires;
      clearInterval(timer);
      timer = setInterval(() => {
        status.textContent = expires
          ? "Session expires in " + Math.max(0, Math.round((expires - Date.now()) / 1000)) + "s"
          : "Open (unsigned)";
      }, 1000);

      const native = document.getElementById("native").checked;
      if (!native && !window.Hls) note("hls.js didn't load (blocked CDN?); trying native HLS");
      if (!native && window.Hls && Hls.isSupported()) {
        note("Using hls.js " + Hls.version);
        // A short buffer, so the 2-minute test clips still fetch after a session
        // expires (otherwise they're fully buffered and expiry is never seen)
        hls = new Hls({ maxBufferLength: 10, maxMaxBufferLength: 20 });
        hls.on(Hls.Events.MANIFEST_PARSED, (_, data) => {
          note("Manifest parsed: " + data.levels.length + " level(s); starting playback");
          video.play().catch((e) => note("play() rejected: " + e.name + " (press play on the video)"));
        });
        hls.on(Hls.Events.LEVEL_SWITCHED, (_, data) => note("Level " + data.level));
        let fragments = 0;
        hls.on(Hls.Events.FRAG_LOADED, () => {
          if (++fragments <= 3) note("Fragment loaded (" + fragments + ")");
        });
        hls.on(Hls.Events.ERROR, (_, data) => {
          // Cross-origin failures carry no CORS headers, so there's usually no status
          const code = data.response && data.response.code;
          const detail = data.type + " / " + data.details + (code ? " (" + code + ")" : " (no readable status)");
          if (!data.fatal) return note("warning " + detail);
          fail("Playback failed: " + detail);
          // Not inside hls.js's own event dispatch
          const failed = hls;
          hls = null;
          setTimeout(() => failed.destroy(), 0);
        });
        hls.loadSource(session.url);
        hls.attachMedia(video);
      } else if (video.canPlayType("application/vnd.apple.mpegurl")) {
        note("Using native HLS");
        video.onerror = () => fail("Playback failed (native): " + (video.error && video.error.code));
        video.src = session.url;
        video.play().catch((e) => note("play() rejected: " + e.name + " (press play on the video)"));
      } else {
        fail("No HLS support in this browser");
      }
    } catch (e) {
      fail(e.message);
    }
  }

  window.addEventListener("error", (e) => note("Page error: " + e.message));
  document.getElementById("play").addEventListener("click", play);
  // Chrome disables the native controls until there's a source, so the video
  // has none until a session starts; clicking it starts one
  video.addEventListener("click", () => {
    if (!video.currentSrc) play();
  });
</script>
</body>
</html>`;

const server = createServer(async (req, res) => {
  try {
    const url = new URL(req.url, "http://localhost");
    if (req.method === "GET" && url.pathname === "/") return send(res, 200, page, "text/html");
    if (req.method === "POST" && url.pathname === "/sessions") return await createSession(req, res);
    const match = url.pathname.match(/^\/s\/([0-9a-f-]{36})\/([^/]+)$/);
    if (req.method === "GET" && match) return await playlist(res, match[1], match[2]);
    send(res, 404, "not found");
  } catch (error) {
    console.error(error);
    send(res, 500, "error");
  }
});

// Exit at once on Ctrl+C (or the terminal closing), dropping the browser's
// keep-alive connections, so the terminal is handed back cleanly everywhere
for (const signal of ["SIGINT", "SIGTERM", "SIGHUP"]) {
  process.on(signal, () => {
    server.close();
    server.closeAllConnections();
    console.log("Stopped");
    process.exit(0);
  });
}

server.listen(Number(values.port), "127.0.0.1", () => {
  console.log(`Test client on http://localhost:${values.port} (Ctrl+C to stop)`);
});
