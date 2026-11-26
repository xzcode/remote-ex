# Third-party distribution review (M5)
 
Status: **incomplete**. A public repository, a package checksum, an ad-hoc signed app and CI success do not establish license compliance or a publisher signature. The project owner has not selected a final project license. No project LICENSE is added by this batch.

## Package evidence
Each current native development workflow writes DEPENDENCIES.json from the actual resolved Cargo and npm lockfiles. In addition to names, versions and available hashes, the new inventory records declared license expressions from corresponding local Cargo registry manifests and the npm lockfile/installed manifest. Missing, uncached, conflicting or local-project declarations are explicitly marked. File-only metadata records presence, not the file path or an inferred license. The inventory includes development and non-target packages; it is not a binary composition SBOM or evidence of which code was linked. It does not download license data.

Review flags: declared_review_pending, copyleft_or_reciprocal_review, special_terms_review, missing_or_file_only. These are triage hints only: expressions containing OR/AND, exceptions and license files require human review. The publisher checks evidence consistency but does not claim legal review passed.

## Newly used libraries and fonts
- fontdb 0.23.0: https://docs.rs/fontdb/0.23.0/fontdb/struct.Database.html ; system-installed font discovery.
- ab_glyph 0.2.32: https://docs.rs/crate/ab_glyph/0.2.32 ; rasterization of fixed local UI labels.
- Fonts are read on the user's device only; no fonts are embedded, copied into packages, downloaded by the app, transmitted or redistributed. Linux CI installs distribution Noto CJK fonts in its disposable runner to validate native labels.
- GStreamer official runtime is still external; source code, plugin/linking and codec obligations remain a separate review. Do not bundle official or third-party runtime installers until redistribution rights and notices have been reviewed.
- Other notable libraries include Tauri, Vue, Winit, Softbuffer, Rustls, XCap, Ashpd, Reqwest, Rfd, cap-std, coturn (separate service), and SQLite. Verify actual resolved licenses rather than assuming a top-level repository license covers dependencies.

## Before stable distribution
1. Owner chooses project license and publication terms.
2. Generate target-specific binary SBOM and review actual statically/dynamically linked code, OS/runtime components, fonts and encoder patents.
3. Collect required license texts, notices, attribution and source/offer obligations. Review build tool distributions separately. Record exceptions and legal decisions.
4. Review source/lockfile reproducibility, vulnerability findings, platform signing/notarization, signed update trust and rollback handling. Do not ship secrets or automatic update signing keys.
5. Complete authorized real-device install/session acceptance and deployment review.

An alpha candidate is still experimental, not proof of completed distribution review.


## M6 browser lab candidate

The isolated apps/web/crypto crate selects Snow 0.10.0's existing NN primitive features (https://docs.rs/crate/snow/0.10.0/features), wasm-bindgen 0.2.106 (https://crates.io/crates/wasm-bindgen), and JS entropy features for getrandom 0.2 and 0.3 (https://docs.rs/getrandom/0.3.4/getrandom/#webassembly-support). Browser WebCrypto generates non-extractable ephemeral fixture signing keys. Playwright 1.56.1/Chromium are CI tooling only; neither browser nor native fixture executable is bundled. Review resolved package graph, notices and WASM distribution obligations before public product release. No audited browser cryptography claim.
