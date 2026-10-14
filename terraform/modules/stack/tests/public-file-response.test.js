const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const vm = require("node:vm");

const functionPath = path.join(
  __dirname,
  "..",
  "assets",
  "cloudfront",
  "public-file-response.js",
);
const source = fs.readFileSync(functionPath, "utf8");

// Terraform prepends the CORS configuration (public_signed.tf)
function load(corsOriginsByPath) {
  const context = {};
  vm.runInNewContext(
    `var CORS_ORIGINS_BY_PATH = ${JSON.stringify(corsOriginsByPath)};\n${source}\nthis.testHandler = handler;`,
    context,
  );
  return context.testHandler;
}

const handler = load({});
const corsHandler = load({
  "/open-hls/": ["http://localhost:8080", "http://localhost:8081"],
  "/restricted/": ["http://localhost:8080"],
});

function run(fn, uri, headers = {}, request = {}) {
  const event = {
    request: { uri, headers: {}, ...request },
    response: {
      statusCode: 200,
      headers: JSON.parse(JSON.stringify(headers)),
      cookies: { session: { value: "unchanged" } },
    },
  };

  return fn(event);
}

function execute(uri, headers = {}, request = {}) {
  return run(handler, uri, headers, request);
}

function executeCors(uri, origin, headers = {}) {
  const request = origin === undefined ? {} : { headers: { origin: { value: origin } } };
  return run(corsHandler, uri, headers, request);
}

function hasHeader(response, name) {
  return Object.prototype.hasOwnProperty.call(response.headers, name);
}

function corsHeaderNames(response) {
  return Object.keys(response.headers).filter((name) => name.startsWith("access-control-"));
}

function header(response, name) {
  return response.headers[name].value;
}

test("forces MIME types for approved inline files", () => {
  const cases = {
    "/documents/report.pdf": "application/pdf",
    "/video/master.M3U8": "application/vnd.apple.mpegurl",
    "/video/segment.ts": "video/mp2t",
    "/video/segment.m4s": "video/iso.segment",
    "/video/movie.mp4": "video/mp4",
    "/subtitles/en.vtt": "text/vtt",
    "/video/encryption.key": "application/octet-stream",
    "/images/poster.JpEg": "image/jpeg",
  };

  for (const [uri, mimeType] of Object.entries(cases)) {
    const response = execute(uri, {
      "content-type": { value: "text/html" },
      "content-disposition": { value: "attachment; filename=evil.html" },
    });

    assert.equal(header(response, "content-type"), mimeType);
    assert.equal(header(response, "content-disposition"), "inline");
  }
});

test("makes active and unknown content download-only", () => {
  const paths = [
    "/page.html",
    "/image.svg",
    "/worker.js",
    "/module.wasm",
    "/feed.xml",
    "/archive.zip",
    "/extensionless",
    "/encoded.%70df",
    "/trailing.pdf/",
  ];

  for (const uri of paths) {
    const response = execute(uri, {
      "content-type": { value: "text/html" },
      "content-disposition": { value: "inline" },
    });

    assert.equal(header(response, "content-type"), "application/octet-stream");
    assert.equal(header(response, "content-disposition"), "attachment");
  }
});

test("uses only the final literal extension", () => {
  const response = execute("/payload.html.pdf");

  assert.equal(header(response, "content-type"), "application/pdf");
  assert.equal(header(response, "content-disposition"), "inline");
});

test("ignores the separate CloudFront query string", () => {
  const response = execute("/movie.mp4", {}, {
    querystring: { download: { value: "html" } },
  });

  assert.equal(header(response, "content-type"), "video/mp4");
});

test("adds browser security headers and preserves unrelated response data", () => {
  const response = execute("/movie.mp4", {
    "content-security-policy": { value: "script-src *" },
    "x-content-type-options": { value: "off" },
    "referrer-policy": { value: "unsafe-url" },
    etag: { value: "original-etag" },
    "content-range": { value: "bytes 0-99/1000" },
  });

  assert.equal(
    header(response, "content-security-policy"),
    "default-src 'none'; media-src 'self'; base-uri 'none'; form-action 'none'",
  );
  assert.equal(header(response, "x-content-type-options"), "nosniff");
  assert.equal(header(response, "referrer-policy"), "no-referrer");
  assert.equal(header(response, "etag"), "original-etag");
  assert.equal(header(response, "content-range"), "bytes 0-99/1000");
  assert.deepEqual(response.cookies, { session: { value: "unchanged" } });
});

test("allows listed origins on each CORS path", () => {
  const cases = [
    ["/open-hls/a/a-playlist.m3u8", "http://localhost:8080"],
    ["/open-hls/a/a-a160k_v4.ts", "http://localhost:8081"],
    ["/restricted/space/a/a-playlist.m3u8", "http://localhost:8080"],
  ];

  for (const [uri, origin] of cases) {
    const response = executeCors(uri, origin, { etag: { value: "original-etag" } });

    assert.equal(header(response, "access-control-allow-origin"), origin);
    assert.equal(header(response, "access-control-expose-headers"), "Accept-Ranges,Content-Range,ETag");
    assert.equal(header(response, "vary"), "Origin");
    assert.equal(header(response, "etag"), "original-etag");
    assert.deepEqual(corsHeaderNames(response).sort(), [
      "access-control-allow-origin",
      "access-control-expose-headers",
    ]);
  }
});

test("gives each path only its own origins", () => {
  const response = executeCors("/restricted/space/a/a-playlist.m3u8", "http://localhost:8081");

  assert.deepEqual(corsHeaderNames(response), []);
  assert.equal(header(response, "vary"), "Origin");
});

test("grants nothing to other, missing, or near-miss origins", () => {
  const origins = [
    "https://evil.example",
    "http://localhost:8082",
    "http://127.0.0.1:8080",
    "http://localhost:8080/",
    "HTTP://LOCALHOST:8080",
    "null",
    "",
    undefined,
  ];

  for (const origin of origins) {
    const response = executeCors("/open-hls/a/a-playlist.m3u8", origin);

    assert.deepEqual(corsHeaderNames(response), [], `origin ${origin}`);
    assert.equal(header(response, "vary"), "Origin");
  }
});

test("adds no CORS or Vary outside the listed paths", () => {
  const uris = [
    "/404.txt",
    "/open-hls",
    "/other/open-hls/a.m3u8",
    "/OPEN-HLS/a.m3u8",
    "/restricted-x/a.m3u8",
  ];

  for (const uri of uris) {
    const response = executeCors(uri, "http://localhost:8080");

    assert.deepEqual(corsHeaderNames(response), [], uri);
    assert.equal(hasHeader(response, "vary"), false, uri);
  }

  const noConfig = execute("/open-hls/a.m3u8", {}, {
    headers: { origin: { value: "http://localhost:8080" } },
  });
  assert.deepEqual(corsHeaderNames(noConfig), []);
  assert.equal(hasHeader(noConfig, "vary"), false);
});

test("removes inherited CORS headers everywhere", () => {
  const inherited = {
    "access-control-allow-origin": { value: "*" },
    "access-control-allow-credentials": { value: "true" },
    "access-control-allow-methods": { value: "GET, PUT, DELETE" },
    "access-control-allow-headers": { value: "*" },
    "access-control-expose-headers": { value: "x-amz-meta-secret" },
    "access-control-max-age": { value: "86400" },
  };

  for (const origin of ["https://evil.example", undefined]) {
    const response = executeCors("/open-hls/a.m3u8", origin, inherited);
    assert.deepEqual(corsHeaderNames(response), []);
  }

  const outside = executeCors("/photos/cat.jpg", "https://evil.example", inherited);
  assert.deepEqual(corsHeaderNames(outside), []);

  const allowed = executeCors("/open-hls/a.m3u8", "http://localhost:8080", inherited);
  assert.deepEqual(corsHeaderNames(allowed).sort(), [
    "access-control-allow-origin",
    "access-control-expose-headers",
  ]);
  assert.equal(header(allowed, "access-control-allow-origin"), "http://localhost:8080");
  assert.equal(header(allowed, "access-control-expose-headers"), "Accept-Ranges,Content-Range,ETag");
});

test("preserves existing Vary values when adding Origin", () => {
  const cases = [
    ["Accept-Encoding", "Accept-Encoding, Origin"],
    ["Accept-Encoding, origin", "Accept-Encoding, origin"],
    ["Origin", "Origin"],
    ["*", "*"],
    ["", "Origin"],
  ];

  for (const origin of ["http://localhost:8080", "https://evil.example", undefined]) {
    for (const [existing, expected] of cases) {
      const response = executeCors("/open-hls/a.ts", origin, { vary: { value: existing } });
      assert.equal(header(response, "vary"), expected, `${origin} / ${existing}`);
    }
  }
});

test("keeps permissions specific to each request for the same cached object", () => {
  // CloudFront caches the origin response and runs the function per viewer,
  // so every request starts from the same headers, including on cache hits.
  const cached = {
    etag: { value: "same-object" },
    "x-cache": { value: "Hit from cloudfront" },
  };
  const sequence = [
    ["http://localhost:8080", "http://localhost:8080"],
    ["http://localhost:8081", "http://localhost:8081"],
    ["https://evil.example", undefined],
    [undefined, undefined],
    ["http://localhost:8080", "http://localhost:8080"],
  ];

  for (const [origin, allowed] of sequence) {
    const response = executeCors("/open-hls/a/a-a160k_v4.ts", origin, cached);

    assert.equal(response.headers["access-control-allow-origin"]?.value, allowed);
    assert.equal(header(response, "vary"), "Origin");
    assert.equal(header(response, "etag"), "same-object");
  }

  // Even if a previous viewer's grant were somehow present, it doesn't carry over
  const leaked = executeCors("/open-hls/a/a-a160k_v4.ts", "https://evil.example", {
    ...cached,
    "access-control-allow-origin": { value: "http://localhost:8080" },
    vary: { value: "Origin" },
  });
  assert.deepEqual(corsHeaderNames(leaked), []);
  assert.equal(header(leaked, "vary"), "Origin");
});

test("applies CORS alongside content type and security headers", () => {
  const response = executeCors("/restricted/space/a/a-playlist.m3u8", "http://localhost:8080", {
    "content-type": { value: "text/html" },
  });

  assert.equal(header(response, "content-type"), "application/vnd.apple.mpegurl");
  assert.equal(header(response, "x-content-type-options"), "nosniff");
  assert.equal(header(response, "access-control-allow-origin"), "http://localhost:8080");
});

test("handles the controlled error object as plain text", () => {
  const response = execute("/404.txt");

  assert.equal(header(response, "content-type"), "text/plain; charset=utf-8");
  assert.equal(header(response, "content-disposition"), "inline");
});

test("allows same-origin playback for browser media documents without sandboxing", () => {
  const extensions = [
    "aac", "m4a", "m4s", "mov", "mp3", "mp4", "ogg", "ts", "wav", "webm", "MP4",
  ];

  for (const extension of extensions) {
    const response = execute(`/media/movie.${extension}`);

    assert.equal(
      header(response, "content-security-policy"),
      "default-src 'none'; media-src 'self'; base-uri 'none'; form-action 'none'",
    );
    assert.equal(header(response, "content-disposition"), "inline");
  }
});

test("allows HLS playlists to load same-origin media and keys", () => {
  for (const uri of ["/hls/master.M3U8", "/hls/720p/playlist.m3u8"]) {
    const response = execute(uri, {
      "content-security-policy": { value: "sandbox; default-src 'none'" },
    });

    assert.equal(
      header(response, "content-security-policy"),
      "default-src 'none'; media-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'none'",
    );
    assert.equal(header(response, "content-type"), "application/vnd.apple.mpegurl");
    assert.equal(header(response, "content-disposition"), "inline");
  }
});

test("retains sandboxing for non-media files, HLS keys, and subtitles", () => {
  const paths = [
    "/page.html",
    "/image.svg",
    "/worker.js",
    "/report.pdf",
    "/image.png",
    "/encryption.key",
    "/en.vtt",
    "/404.txt",
    "/movie.mp4.html",
    "/encoded.%6dp4",
    "/movie.mp4/",
    "/extensionless",
  ];

  for (const uri of paths) {
    const response = execute(uri, {}, {
      querystring: { filename: { value: "movie.mp4" } },
    });

    assert.equal(
      header(response, "content-security-policy"),
      "sandbox; default-src 'none'; base-uri 'none'; form-action 'none'",
    );
  }
});
