# Client implementation

How a client application plays HLS audio and video through [signed delivery](signed-delivery.md): what HLS is, why signing needs work on the client side, and what a client has to build. The test client in [`scripts/cf-signed-test`](https://github.com/duracloud/duracloud-preserve/tree/main/scripts/cf-signed-test) (`client.mjs`) is used as the example throughout. It's a test harness, not a reference implementation: its sessions are kept in memory and its authorization is a stub.

## HLS basics

HLS (HTTP Live Streaming) doesn't serve a video as one file. The video is cut into short chunks called **segments**, usually about 6 seconds each and usually `.ts` files. Plain-text **playlists** (`.m3u8`) tell the player which segments exist. There are two levels of playlist:

**Master playlist** (`TEST_C_0002-playlist.m3u8`): one entry per quality level.

```
#EXT-X-STREAM-INF:BANDWIDTH=590821,RESOLUTION=640x360,...
TEST_C_0002-a160k_v4.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=289269,RESOLUTION=320x180,...
TEST_C_0002-a96k_v4.m3u8
```

**Variant (media) playlist**: the segments for one quality, in order.

```
#EXTINF:6.000000,
TEST_C_0002-a160k_v4-00001.ts
#EXTINF:6.000000,
TEST_C_0002-a160k_v4-00002.ts
```

A player works like this:

1. It fetches the master playlist and picks a quality.
2. It fetches that quality's variant playlist.
3. It downloads segments in order, staying a little ahead of the playhead (the buffer).
4. If bandwidth changes, it switches to another variant's playlist and continues from the next segment. This is "adaptive bitrate."
5. To seek, it works out which segment holds that time and fetches it.

Some content packs all segments into one `.ts` file and lists byte ranges instead (`#EXT-X-BYTERANGE:430708@438980`). The player then sends HTTP `Range` requests and gets `206` responses.

It's all ordinary HTTP GETs of static files. That's why HLS fits S3 and CloudFront so well, and also why signing is awkward: **one playback is dozens or hundreds of separate requests, and every one needs a valid signature.**

## Why playlists have to be rewritten

Playlists contain bare relative filenames such as `TEST_C_0002-a160k_v4-00001.ts`. A player resolves those against the playlist's URL, and the result **doesn't keep the query string**. Even if the master playlist URL is signed, the segment requests go out unsigned and are denied.

So the client rewrites each playlist before the player sees it, turning every reference into a fully signed URL. That's the main job on the client side.

A **custom policy with a wildcard** makes this practical. One signature covers `https://<host>/restricted/<space>/<prefix>/*`, so a single set of query parameters (`Policy`, `Signature`, `Key-Pair-Id`, `Hash-Algorithm`) works for every file in that directory. That's why each family of files (one item's playlists and segments) lives in its own directory.

## Request flow

The test client has two parts: a small Node server, standing in for the client application's backend, and a browser page with a player.

| Step | Browser requests | Sent to | What happens |
| ---- | ---------------- | ------- | ------------ |
| 1 | `POST /sessions` | Client server | Authorizes the viewer, signs a policy for the family's directory (`…/<prefix>/*`), and returns a playlist URL on the client server: `/s/<sid>/<prefix>-playlist.m3u8` |
| 2 | `GET /s/<sid>/<prefix>-playlist.m3u8` | Client server | Fetches the master playlist from CloudFront with the signature, rewrites it, and returns it (`no-store`) |
| 3 | `GET /s/<sid>/<prefix>-a160k_v4.m3u8` | Client server | Same as step 2, for the variant playlist the player chose |
| 4 | `GET https://<host>/…/<prefix>-a160k_v4-00001.ts?<signature>` | CloudFront | Checks the signature and returns the segment, with CORS headers |
| 5 | `GET https://<host>/…/<prefix>-a160k_v4-00002.ts?<signature>`, and so on | CloudFront | Same as step 4, for each following segment |

1. **Start a session.** The page asks the server for a specific family. A real application checks here whether the logged-in user may watch it; the test client only checks that the family exists. For open (unsigned) content it returns the plain public URL. For restricted content it:
   - signs a policy for `<base path><space>/<prefix>/*` that expires after the session lifetime
   - stores the signature under a random session ID
   - returns a playlist URL **on its own server**, `/s/<sid>/<prefix>-playlist.m3u8`

2. **Serve playlists.** When the player asks for a playlist, the server fetches the real one from CloudFront using the signature. Because that's a server-side request, it must send a `User-Agent`, or a web ACL may block it. The server then rewrites every reference:
   - **Another `.m3u8`** (the variant playlists) points back to the client server (`/s/<sid>/…`), so it gets rewritten too.
   - **Anything else** (segments, and in real content also encryption keys, subtitles, and alternate audio) becomes an absolute CloudFront URL with the signature attached.
   - `URI="…"` attributes inside tags are rewritten too, not just bare lines.
   - Any reference that isn't a bare filename is rejected, so a playlist can't point outside the directory the signature covers.
   - The result is returned with `Cache-Control: private, no-store`, because it now contains a live signature.

3. **Play.** The player fetches playlists through the client server, but **downloads segments directly from CloudFront**. Only the small text files pass through the client; the media bytes come from the CDN.

## Example: what the player sees

### The page gets one URL

The page hands the player a single URL: the playlist on the client server. In the test client:

```js
const res = await fetch("/sessions", { method: "POST", body: ... });
const session = await res.json();   // { url: "/s/3f9c…/TEST_C_0002-playlist.m3u8", expires: … }

hls.loadSource(session.url);        // hls.js
hls.attachMedia(video);
// or, native HLS (Safari):
video.src = session.url;
```

A real application might render the URL straight into the page's HTML instead, but the result is the same: the player points at a playlist URL on the client's server, not at CloudFront. Every other URL comes from the rewritten playlists.

### The master playlist is rewritten on the fly

The player requests `GET /s/3f9c…/TEST_C_0002-playlist.m3u8`. The client server fetches the original from CloudFront (signed), rewrites the text, and returns it as the response body with `Content-Type: application/vnd.apple.mpegurl` and `no-store`. Nothing is saved anywhere. The rewritten playlist exists only in that response, and it's regenerated every time the player asks for it.

Original master playlist:

```
#EXT-X-STREAM-INF:BANDWIDTH=590821,RESOLUTION=640x360,...
TEST_C_0002-a160k_v4.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=289269,RESOLUTION=320x180,...
TEST_C_0002-a96k_v4.m3u8
```

What the player receives:

```
#EXT-X-STREAM-INF:BANDWIDTH=590821,RESOLUTION=640x360,...
/s/3f9c…/TEST_C_0002-a160k_v4.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=289269,RESOLUTION=320x180,...
/s/3f9c…/TEST_C_0002-a96k_v4.m3u8
```

The variant playlists point back to the client server, so they get the same treatment.

### The variant playlist gets signed CloudFront URLs

The player picks a quality and requests `/s/3f9c…/TEST_C_0002-a160k_v4.m3u8`. The client server fetches and rewrites it the same way, but this time the references are segments.

Original:

```
#EXTINF:6.000000,
TEST_C_0002-a160k_v4-00001.ts
#EXTINF:6.000000,
TEST_C_0002-a160k_v4-00002.ts
```

What the player receives:

```
#EXTINF:6.000000,
https://<host>/restricted/campus-hls/TEST_C_0002/TEST_C_0002-a160k_v4-00001.ts?Policy=eyJTdGF0…&Signature=Qm1x…&Key-Pair-Id=K2ABC…&Hash-Algorithm=SHA256
#EXTINF:6.000000,
https://<host>/restricted/campus-hls/TEST_C_0002/TEST_C_0002-a160k_v4-00002.ts?Policy=eyJTdGF0…&Signature=Qm1x…&Key-Pair-Id=K2ABC…&Hash-Algorithm=SHA256
```

Every line has the same query string. Only the filename changes, because the one wildcard policy covers the whole directory. For byte-range content, each entry points at the same signed `.ts` URL, the `#EXT-X-BYTERANGE` tags pass through unchanged, and the player sends a `Range` header with each request.

### The player fetches segments itself

The player follows the URLs it was given. It requests each segment directly from CloudFront, CloudFront checks the signature, and the bytes stream to the browser. The client server sees only the few playlist requests: at the start, and again when the player switches quality and loads another variant playlist.

The page JavaScript never builds a signed URL or handles the private key. The signature reaches the browser only inside the rewritten URLs, so anyone who copies one gets a URL that works until it expires, and only for that one directory.

## The player

There are two ways to play HLS in a browser:

- **hls.js.** This JavaScript library downloads playlists and segments with `fetch` or XHR and feeds the data to the `<video>` element through Media Source Extensions. It's needed in browsers without native HLS. Because the page's JavaScript makes cross-origin requests (the page is on the client's domain, the media on the stack's), **CORS headers are required**. The stack's per-path CORS settings provide them.
- **Native HLS** (Safari, and every browser on iOS). The page sets `video.src` to the playlist URL and the browser handles the rest. JavaScript has no hook into those requests.

The second point is why rewriting happens on the server. With hls.js the signature could, in principle, be attached in the browser through a custom loader, but native HLS gives no way to change requests. A rewritten playlist works with any player.

Also:

- The player's requests are simple GETs (plus `Range`, which browsers allow without extra checks), so the browser never sends a CORS **preflight**. A preflight can't carry a signature and would be denied.
- The test client sets a deliberately short buffer (`maxBufferLength: 10`) so that expiry shows up during testing.

## Expiry and errors

The signature has a hard expiry (`DateLessThan` in the policy). Segments already in the buffer keep playing, but the next fetch after expiry is denied. How the denial looks to hls.js depends on which request hit it:

- **Segments** go directly to CloudFront from another origin. The denial arrives as 404 without CORS headers, so hls.js sees only a network error with no readable status.
- **Playlists** go through the client server, on the page's own origin. When CloudFront denies the client server's fetch, the test client passes the 404 on, and hls.js can read it. After expiry this happens when the player loads a new variant playlist, for example when it switches quality.

Either way, the client can't tell an expired signature from any other denial, because CloudFront returns 404 for all of them.

The test client just shows an error and asks for a new session. A real client should:

1. treat any failed request as possibly expired
2. request a new session (re-authorize) once
3. resume at the current position
4. show an error if that also fails

Choosing the session lifetime is a trade-off:

- **Long enough** to cover a viewing, including pauses, without frequent re-authorizing.
- **Short enough** that a leaked URL isn't useful for long. Signed URLs stay valid after logout; only removing the key revokes them early.

Short audio-only files can buffer completely, so the player may never make a request after expiry. Expiry then never comes up for that session.

## What a client needs to build

**From the infrastructure team, the client receives:**

- the signed base URL (`cloudfront_signed_base_url`)
- its key ID (from `cloudfront_signed_key_ids`)

**To the infrastructure team, the client sends:**

- its public key (PEM)
- its player origins for each path, for CORS

**The client builds:**

1. **An authorization endpoint.** It decides whether this user may play this item, then signs one custom policy for the item's directory (`…/<prefix>/*`) with the client's private key, which never leaves the client's server. AWS SDKs do the signing (`@aws-sdk/cloudfront-signer` in the test client, with equivalents for Python, Ruby, Java, and others).
2. **A playlist proxy and rewriter.** It fetches each `.m3u8` with the signature (sending a `User-Agent`) and rewrites:
   - child playlists to point back to itself
   - everything else to a signed CloudFront URL

   It also handles `URI="…"` attributes and returns the result as non-cacheable.
3. **A player.** hls.js where the browser lacks native HLS, native HLS on Safari and iOS. Nothing about signing lives in the player.
4. **Error handling.** On any playback network error, re-authorize once, then fail.

For **open** content, steps 1 and 2 are skipped: the player points straight at the public playlist URL. CORS still applies if the player uses hls.js.

The rules the client must follow are listed in the [client contract](signed-delivery.md#client-contract).
