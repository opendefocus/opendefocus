# Security policy

## Supported versions

Security fixes go into the latest release of each package (`opendefocus`, the
OpenFX plugin, `circle-of-confusion`, `bokeh-creator`, `image-ndarray`). Older
releases are not patched.

## Reporting a vulnerability

Report privately through GitHub: open the repository's **Security** tab and
choose **Report a vulnerability**. Do not open a public issue for a
vulnerability.

Include the affected package and version, the host application if any, steps or
an input file that reproduces it, and the impact you expect. Fixes are released
before details are published, and reporters are credited unless they ask not to
be.

## Scope

In scope: memory safety in the OFX plugin and Python bindings, crashes or
unbounded memory or GPU allocation triggered by host input or settings, and the
build and release pipeline.

Out of scope: bugs in host applications, GPU drivers or WGPU itself (report
those upstream), and render quality issues (open a normal issue).

## Rules for contributors

- The plugin runs inside host processes, so a panic or out-of-bounds access
  crashes the user's application. Treat host pointers, image bounds, row
  strides and parameter values as untrusted: validate them and return an error.
- Every `unsafe` block has a `// SAFETY:` comment stating the invariant it
  relies on. Keep `unsafe` in the FFI layer (`opendefocus-ofx`) and its tests.
- Check dimensions and GPU limits before allocating, as the renderer already
  does; see [ARCHITECTURE.md](ARCHITECTURE.md).
- New dependencies must pass `cargo audit` and dependency review in CI. Prefer
  existing dependencies for small tasks.
- Workflows request the least `permissions` they need. Never commit
  credentials or signing material.

## Personal information

Commits, issues, pull requests, comments, fixtures and logs are public and stay
public. Keep personal information out of all of them:

- Use repository-relative paths, never absolute home paths (`/Users/<name>/`,
  `/home/<name>/`, `C:\Users\<name>\`). Redact usernames, hostnames, IP
  addresses, serial numbers and emails from pasted logs, backtraces and reports.
  Adapter and GPU names are fine.
- Commit with your GitHub noreply address (`<id>+<user>@users.noreply.github.com`).
- Strip metadata from committed images: EXR `owner`, `hostName` and `comments`
  attributes, EXIF, GPS and XMP. Do not commit footage you do not have the
  rights to publish.
- Reproduction files from bug reports and vulnerability reports stay outside
  Git unless the reporter agrees to publish them.
