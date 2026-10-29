# Static files of forge-web

- `htmx.min.js`: htmx 2.0.4 from `https://unpkg.com/htmx.org@2.0.4/dist/htmx.min.js`,
  SHA-256 `e209dda5c8235479f3166defc7750e1dbcd5a5c1808b7792fc2e6733768fb447`. Vendored, so the
  site needs no CDN and the Content Security Policy can stay at `script-src 'self'`.
- `app.css`: the site's own styles.

Both are compiled into the binary.
