# Signed delivery

A stack's CloudFront distribution can require [signed URLs](https://docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/private-content-signed-urls.html) for one path prefix in its public bucket (default `/restricted/`). The client's application authenticates and authorizes viewers and signs URLs; DuraCloud Preserve registers the client's public keys and serves the content. CORS can be enabled per path prefix for browser players. See [Signed delivery and CORS](../deploy/README.md#signed-delivery-and-cors) for the stack inputs.

## How it works

- An ordered cache behavior on `<base path>*` uses the existing S3 origin (through OAC) and trusts a key group containing the configured public keys. CloudFront validates the signature on every request, including cache hits, and on the `*.cloudfront.net` hostname as well as the custom domain.
- Signature parameters are not part of the cache key (`Managed-CachingOptimized`), so viewers with different signatures share cached objects.
- Each CORS path gets a response headers policy with its own origin list: `GET`/`HEAD`, no credentials, exposing `Accept-Ranges`, `Content-Range`, and `ETag`. Paths not listed, including the default behavior, get no CORS headers. The signed path's entry attaches to the signed behavior; every other entry gets its own unsigned behavior.
- All behaviors keep the existing viewer-response function (content types, `Content-Disposition`, security headers).
- Missing objects and failed signatures can both reach viewers as 404, because the distribution maps 403 to `/404.txt`. Clients should treat 403 and 404 alike on signed URLs: re-authorize once, then show an error.

## Client contract

Give the client these rules along with the base URL and key ID:

- Generate an RSA 2048 key pair, keep the private key, and send only the public PEM.
- Sign a custom policy with SHA-256 (for example `@aws-sdk/cloudfront-signer` with `algorithm: "SHA256"`), scoped to one content directory: `https://<host><base path><dir>/*`. Never sign the whole base path or anything wider than the viewer is authorized for. CloudFront doesn't enforce this.
- Sign once per playback session and reuse the `Policy`, `Signature`, `Key-Pair-Id`, and `Hash-Algorithm` parameters on every URL in the directory.
- The policy must name the hostname the browser requests.
- Players must not trigger CORS preflights (no custom headers, no credentials). A preflight `OPTIONS` request can't carry a signature and is denied.
- Signed URLs stay valid until they expire, even after the viewer logs out. Removing the key revokes them.

## Onboarding

1. The client sends its public PEM and the exact player origins for each path.
2. List the public bucket under the base path. Anything already there stops being public once signing is enabled.
3. Add the key to `cloudfront_signed_public_keys`, set `cloudfront_signed_enabled`, add the CORS paths, and apply.
4. Check that an unsigned request for a test object under the base path is denied, and a signed one succeeds.
5. Send the client `cloudfront_signed_base_url` and its key ID from `cloudfront_signed_key_ids`.

If content is already being served under the base path, register the key in an earlier apply and send the client its key ID first. Otherwise that content is unreachable from the enabling apply until the client starts signing.

## Key rotation

1. The client generates a new key pair and sends the public PEM.
2. Add it under a new name in `cloudfront_signed_public_keys` and apply. Send the client the new key ID.
3. The client signs new sessions with the new key.
4. Wait for the longest playback session signed with the old key to expire.
5. Remove the old key and apply.

Up to 5 keys can be registered at once. Replacing the PEM under an existing name also works: the new key is created before the old one is removed.

## Compromised key

Remove the key from `cloudfront_signed_public_keys` and apply. Every URL signed with it stops working once the distribution change deploys (usually minutes). Signing can't be left enabled with no keys, so register the replacement in the same apply if it's the only key.

## Never disable to fix signing

Disabling signed delivery or changing the base path makes everything under the old prefix public through the default behavior. Move or delete that content first, and invalidate cached copies.

## Quotas

By default CloudFront allows 25 cache behaviors per distribution and 20 custom response headers policies per account. Each CORS path uses one policy, which is fine with one stack per account. If several stacks share an account, watch the policy count.
