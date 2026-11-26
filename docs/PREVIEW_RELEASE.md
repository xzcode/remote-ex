# Preview release procedure

The owner approved the first alpha preview tag `v0.1.0-alpha.1`. A preview release is not a completion claim for M4, an OS-signed installer, or deployment of a public server.

## Explicit release intent and verification
- `.github/preview-release.json` declares the intended alpha tag, `prerelease: true` and an attempt number. Change that file only with authorization to publish a new version or retry an unreleased candidate.
- A push of the marker on master triggers Core CI, Desktop checks, Native input checks, Native media engine, Desktop development builds (Windows/macOS), and Linux X11 development candidate. The publisher responds to their completion, including when they complete in a different order. It skips superseded candidates, PRs, and failures. No new build is started by the publisher.
- `preview-release.yml` checks that every required run succeeded on the exact master revision that changed the marker. It downloads the three platform-specific artifacts from those runs, checks expected binaries and commit/architecture stamps, produces SHA256SUMS.txt, and creates a GitHub prerelease. It never replaces a published asset or moves an existing tag.
- If a build fails, correct the actual problem, increment the attempt in the marker, and push the corrected candidate. All checks must match the new commit. The manual workflow dispatch can re-evaluate the same current candidate if all checks succeeded but publishing was interrupted. Inspect an existing draft/tag before retrying; the workflow will not silently modify it.
- The publisher uses the repository's ephemeral GitHub Actions token with job-scoped contents write/actions read. No personal access token or external deployment secret is required. Repository workflow permissions/rules may still deny release creation; inspect the actual workflow conclusion.
- Actual physical display capture/input, Wayland portal interoperability, lock/suspend handling, power behavior, and application installation are not established by CI.

## Archives and external runtime
Windows x64 ZIP, macOS ARM64 tar.gz (ad-hoc signed app, no notarization), and Linux x64 debug development tar.gz. Each includes the self-host rendezvous binary and build documentation; none bundles the official GStreamer runtime. Refer to DEV_BUILD.md for dependencies and normal OS permissions. Do not select a final project license without owner approval or represent unverified third-party distribution obligations as resolved.

Publication is complete only when the Release page contains the tagged prerelease and all three archives plus SHA256SUMS.txt. Temporary Actions artifact links alone are not a release.
