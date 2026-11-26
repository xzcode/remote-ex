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

## First publication record — 2026-11-26

- Release: [v0.1.0-alpha.1](https://github.com/xzcode/remote-ex/releases/tag/v0.1.0-alpha.1), published 03:07:19 UTC; prerelease, not draft.
- Tag and release target: `303163ce2e85aea2f00df6f340ec932a6cfb29c0`.
- All six source-commit checks passed: [Core](https://github.com/xzcode/remote-ex/actions/runs/45745271481), [Desktop](https://github.com/xzcode/remote-ex/actions/runs/45745271612), [Native input](https://github.com/xzcode/remote-ex/actions/runs/45745271609), [Native media](https://github.com/xzcode/remote-ex/actions/runs/45745271569), [Windows/macOS](https://github.com/xzcode/remote-ex/actions/runs/45745271487), [Linux](https://github.com/xzcode/remote-ex/actions/runs/45745271505).
- [Publisher run](https://github.com/xzcode/remote-ex/actions/runs/45746815364): source checks, archive retrieval, archive inspection, commit stamps, checksums and prerelease publication succeeded. GitHub API confirms all four assets are uploaded.
- Archive sizes in bytes: Windows 15036230; macOS 15640368; Linux 171709309. The release includes SHA256SUMS.txt.
- Linux portal compiler fixes are covered by the successful Linux run. Xvfb capture/input fixtures and the bounded portal pipeline tests are not physical Wayland or hardware acceptance.
- Release remains experimental. No server deployment, production endorsement, or OS signing/notarization is implied. This record does not modify the existing tag or assets.

## Next-release package gate
Future explicitly approved previews require six platform formats (three portable archives plus Windows Setup.exe, macOS DMG and Linux DEB), checked component manifests, exact release-tag and revision stamps, dependency input inventory, and SHA256SUMS.txt covering all six packages. Installer smoke steps run in native build jobs. No existing release assets are modified. New gate has not yet been verified by a subsequent release. External GStreamer remains separate and publisher signing/license audit remain incomplete.


## Expanded preview gate
Future explicitly approved previews must also pass Signaling container preflight on the exact candidate revision. Package inventory must include internally consistent declared-license evidence and review flags (not an approval of the licenses). Existing alpha.1 remains immutable. See [third-party review](THIRD_PARTY_REVIEW.md).


## alpha.2 candidate

Explicitly requested v0.1.0-alpha.2. The preceding Linux, media, core and container checks passed; desktop dictionary invocation failed due to the working directory and is corrected in this candidate. Publication awaits all seven new same-commit runs. Native jobs produce PACKAGE_EVIDENCE.json after package smoke checks. The publisher matches each file's SHA-256 and byte count against its exact native build evidence, verifies archive manifests/metadata/architecture and writes RELEASE_BUILD.json with seven successful run IDs and six package hashes. SHA256SUMS.txt includes all six packages and RELEASE_BUILD.json. The public prerelease must have eight assets (six packages plus two evidence/checksum files). Existing alpha.1 remains immutable; no signing, licensing or physical-device acceptance claimed. See M5_ACCEPTANCE.md.


## alpha.2 publication verified — 2026-11-26

[v0.1.0-alpha.2](https://github.com/xzcode/remote-ex/releases/tag/v0.1.0-alpha.2) was published at 06:32:30 UTC (14:32:30 Malaysia time), a published prerelease, not a draft. The Git tag and release target both point to `6a42d6e1fb85d6ecee90a2486e01ac165ead14cf`. All seven exact-commit runs succeeded: [Core](https://github.com/xzcode/remote-ex/actions/runs/45766782091), [Desktop](https://github.com/xzcode/remote-ex/actions/runs/45766782111), [Native input](https://github.com/xzcode/remote-ex/actions/runs/45766782214), [Native media](https://github.com/xzcode/remote-ex/actions/runs/45766782370), [Windows/macOS](https://github.com/xzcode/remote-ex/actions/runs/45766782137), [Linux](https://github.com/xzcode/remote-ex/actions/runs/45766782074), [Container](https://github.com/xzcode/remote-ex/actions/runs/45766782115). [Publisher](https://github.com/xzcode/remote-ex/actions/runs/45768863134) passed every step. GitHub API confirms eight uploaded assets (Windows Setup 7,837,600 bytes and ZIP 16,225,272; macOS DMG 18,262,344 and TAR.GZ 16,769,943; Linux DEB 25,822,306 and TAR.GZ 37,927,547; SHA256SUMS.txt and RELEASE_BUILD.json). The physical acceptance, OS publisher signing and third-party license review gates remain open. No public deployment occurred. No existing tag or asset has been changed.
