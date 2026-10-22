# Launcher development

Use the repository-local author identity: `Dragon Soul Reborn <noreply@dragon-soul-reborn.invalid>`.
Check the existing remote and account-specific credential setting before pushes; do not change global Git or account settings.

Authenticate public repository writes through the Dragon Soul Reborn GitHub App (`dragon-soul-reborn[bot]`). Do not use personal or corporate credentials for pushes, pull requests, workflow dispatches or release operations. Maintainers use a private local bot helper; preserve its repository-local push guard and stop if bot authentication is unavailable. Public branding and metadata must identify Dragon Soul Reborn without personal account names or personal email addresses.

`main` is production. Develop on work branches from `staging`. Production merges, tags and publication require the owner's explicit release instruction.

Never add original game files, account data, private keys, passwords or private filesystem paths. All test fixtures must be synthetic. The installer is read with innoextract and is never executed by the launcher.

Run targeted tests while editing, then `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `pnpm -C launcher test`, and `pnpm -C launcher build` before review.

