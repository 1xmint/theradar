// SPDX-License-Identifier: Apache-2.0
//! The compiled interface, served from inside the binary.
//!
//! Embedding rather than shipping a directory is what keeps a deploy one file.
//! The box has no Node toolchain and should not gain one: the build happens in
//! CI, and the runbook's `sha256sum` discipline covers the interface for free,
//! because it is *inside* the artifact being hashed.
//!
//! # A missing build is a normal state
//!
//! `web/dist` is a build artifact, so a fresh checkout has an empty one. The
//! crate still has to compile and the server still has to start — a developer
//! working on the backend must not be required to run `npm` first. So an absent
//! interface is answered with a page saying how to build it, rather than a 404
//! that reads like a broken deploy.

use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};

/// The Vite build output.
#[derive(rust_embed::Embed)]
#[folder = "../../web/dist/"]
struct Assets;

/// Serves a built asset, or the application shell.
///
/// Anything that is not a file falls through to `index.html`, because the
/// router owns the paths and the server cannot know them. A request for a real
/// asset that is missing would otherwise be answered with the shell, which is
/// why hashed asset names matter: `/assets/index-CxPhWFgt.js` either exists or
/// the deploy is broken, and it will never be a route.
pub fn serve(path: &str) -> Response {
    let mut response = asset_or_shell(path);
    // Cloudflare can proxy directly to this process. Headers present only in
    // Caddy's config do not protect that path (observed 2026-10-06).
    let headers = response.headers_mut();
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(CSP),
    );
    response
}

/// Privy's documented web-client origins, with scripts still restricted to
/// this bundle and Turnstile. Inline styles are needed by the wallet UI;
/// HTTPS fetches and images preserve CoinImage's creator-hosted metadata.
/// X-Frame-Options/frame-ancestors govern who embeds Radar; frame-src governs
/// the wallet iframe Radar embeds. They must not be confused.
const CSP: &str = "default-src 'none'; script-src 'self' https://challenges.cloudflare.com; \
    style-src 'self' 'unsafe-inline'; \
    connect-src 'self' https: wss://relay.walletconnect.com \
    wss://relay.walletconnect.org wss://www.walletlink.org https://*.rpc.privy.systems \
    https://explorer-api.walletconnect.com https://api.mainnet-beta.solana.com; \
    frame-src https://auth.privy.io https://verify.walletconnect.com \
    https://verify.walletconnect.org https://challenges.cloudflare.com; \
    child-src https://auth.privy.io https://verify.walletconnect.com https://verify.walletconnect.org; \
    worker-src 'self'; img-src 'self' https: data: blob:; font-src 'self'; \
    form-action 'none'; frame-ancestors 'none'; base-uri 'none'; object-src 'none'; manifest-src 'self'";

fn asset_or_shell(path: &str) -> Response {
    let trimmed = path.trim_start_matches('/');
    if let Some(asset) = Assets::get(trimmed) {
        let mime = mime_of(trimmed);
        return (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, mime),
                // Content-hashed by Vite, so an asset is immutable: a change
                // produces a different name. The shell below is the opposite.
                (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
            ],
            asset.data.into_owned(),
        )
            .into_response();
    }
    shell()
}

/// The application shell, or an explanation of why there is not one.
fn shell() -> Response {
    let Some(index) = Assets::get("index.html") else {
        return (StatusCode::OK, Html(NOT_BUILT.to_owned())).into_response();
    };
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            // Never cached. The shell names the hashed assets, so a cached one
            // points at a build that is gone -- a blank page after every deploy,
            // fixed by a hard refresh nobody knows to do.
            (header::CACHE_CONTROL, "no-cache"),
        ],
        index.data.into_owned(),
    )
        .into_response()
}

/// What to say when the interface was never built.
const NOT_BUILT: &str = "<!doctype html><meta charset=\"utf-8\"><title>Radar</title>\
<body style=\"font:14px/1.6 ui-monospace,monospace;max-width:40rem;margin:4rem auto;padding:0 1rem\">\
<h1 style=\"font-size:1.1rem\">Radar — interface not built</h1>\
<p>The JSON API is running. The interface is a build artifact and this binary was \
compiled without one.</p>\
<pre style=\"background:#111;color:#eee;padding:1rem;border-radius:6px\">cd web &amp;&amp; npm ci &amp;&amp; npm run build</pre>\
<p>Then rebuild <code>radar-serve</code>. In CI this happens automatically; \
locally it is optional, so that backend work does not require Node.</p>";

/// The content type for a built asset.
///
/// A short table rather than a dependency: Vite emits a handful of extensions
/// and anything outside them is a change somebody made deliberately. Guessing
/// wrong matters — a stylesheet served as `text/plain` is silently ignored by
/// every browser, which looks like a CSS bug rather than a server one.
fn mime_of(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json" | "map") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_application_responses_protect_the_shell_and_allow_the_wallet_iframe() {
        for path in ["/", "/index.html", "/automation", "/token/example"] {
            let response = serve(path);
            assert_eq!(response.headers()[header::X_FRAME_OPTIONS], "DENY");
            assert_eq!(
                response.headers()[header::X_CONTENT_TYPE_OPTIONS],
                "nosniff"
            );
            let policy = response.headers()[header::CONTENT_SECURITY_POLICY]
                .to_str()
                .expect("CSP");
            let directives: std::collections::BTreeMap<_, _> = policy
                .split(';')
                .map(str::trim)
                .filter_map(|part| part.split_once(' '))
                .collect();
            assert_eq!(directives["frame-ancestors"], "'none'");
            assert!(
                directives["frame-src"]
                    .split_whitespace()
                    .any(|origin| origin == "https://auth.privy.io")
            );
            assert!(
                directives["connect-src"]
                    .split_whitespace()
                    .any(|origin| origin == "https:")
            );
            assert!(
                directives["img-src"]
                    .split_whitespace()
                    .any(|origin| origin == "https:")
            );
            assert!(!directives["script-src"].contains("unsafe"));
            assert!(
                !directives["script-src"]
                    .split_whitespace()
                    .any(|origin| origin == "*" || origin == "https:")
            );
        }
    }

    #[test]
    fn a_stylesheet_is_never_served_as_something_a_browser_ignores() {
        // A CSS file with the wrong content type is dropped silently by every
        // browser, and the symptom is an unstyled page that looks like a build
        // problem rather than a serving one.
        assert_eq!(mime_of("assets/index-abc.css"), "text/css; charset=utf-8");
        assert_eq!(
            mime_of("assets/index-abc.js"),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(mime_of("index.html"), "text/html; charset=utf-8");
    }

    #[test]
    fn every_type_vite_can_emit_is_named_rather_than_guessed() {
        // Each arm is a claim that a browser will handle the file correctly, and
        // a deleted arm falls through to `application/octet-stream` -- which,
        // with `nosniff` on, means the browser refuses to use it at all. The
        // symptom is a missing font or a broken image, and the cause is a table
        // nobody tested.
        //
        // Table-driven because the failure is per-arm: eight of these were
        // deleted one at a time by mutation testing and none of them failed a
        // suite that only checked three.
        for (path, expected) in [
            ("index.html", "text/html; charset=utf-8"),
            ("assets/app.js", "text/javascript; charset=utf-8"),
            ("assets/app.mjs", "text/javascript; charset=utf-8"),
            ("assets/app.css", "text/css; charset=utf-8"),
            ("manifest.json", "application/json"),
            ("assets/app.js.map", "application/json"),
            ("icon.svg", "image/svg+xml"),
            ("shot.png", "image/png"),
            ("shot.jpg", "image/jpeg"),
            ("shot.jpeg", "image/jpeg"),
            ("shot.webp", "image/webp"),
            ("favicon.ico", "image/x-icon"),
            ("inter.woff2", "font/woff2"),
        ] {
            assert_eq!(mime_of(path), expected, "wrong type for {path}");
        }
    }

    #[test]
    fn an_unknown_extension_is_a_download_rather_than_a_guess() {
        // Guessing wrong is worse than declining to guess: a mistyped script is
        // executed as whatever the browser sniffs, and `nosniff` is on.
        assert_eq!(mime_of("weird.xyz"), "application/octet-stream");
        assert_eq!(mime_of("noextension"), "application/octet-stream");
    }

    #[test]
    fn a_hashed_name_is_matched_on_its_real_extension() {
        // Vite's names carry a hash before the extension, and splitting on the
        // first dot rather than the last would classify every one of them as
        // unknown.
        assert_eq!(
            mime_of("assets/index-CxPhWFgt.js"),
            "text/javascript; charset=utf-8"
        );
    }
}
