# Releasing the launcher

Development branches start from `staging`; `main` contains production. The owner authorizes production merges and release tags.

Before a release:

1. Run the checks in `AGENTS.md`. The Inno integration test must run with `DSR_INNOEXTRACT` and `DSR_E2E_INSTALLER`; the fixture installer contains synthetic files only.
2. Validate the feed with `cargo run -p recipe-tool -- check-feed --dir publish` and verify the game recipe against a known working installation.
3. Configure repository Actions secrets `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, and `VT_API_KEY`. Enter secrets privately; never commit them. The updater public key in `launcher/src-tauri/tauri.conf.json` must match the private key. Back up the private key and its password offline.
4. Set GitHub Pages to deploy from GitHub Actions. Keep the owner's public email private and use the repository-local project identity for commits.
5. Open the implementation PR into `staging`; obtain a green CI run. After the owner's release instruction, merge `staging` into `main`.
6. Tag a production commit as `v1.0.0` for the first release. The release workflow creates a draft, verifies its signature, scans the installer with VirusTotal, then publishes it only if those gates succeed.
7. Perform the real Windows install, interrupted-download, repair, gameplay and self-update acceptance run. Synthetic integration tests do not replace that final player test.

The data feed contains only filenames, sizes, fingerprints and server settings. Original game bytes are downloaded directly from archive.org or read from a player-supplied copy. The original installer is unpacked, never executed.

The initial server address is loopback for local testing. Public server hosting, account registration and a website are separate work.
