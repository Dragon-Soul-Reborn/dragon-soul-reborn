# Dragon Soul Reborn launcher

The launcher for **Dragon Soul Reborn**, a free community server for Dragon Soul (2011).

- It downloads the original game installer and the 0.27 update files from
  [archive.org](https://archive.org/details/Dragon-Soul-DragonSoul-MMORPG-Client), checks every file against a
  published fingerprint, and sets the game up in a folder you choose.
- This project never hosts the game's files. Everything from the original publisher comes from archive.org, or
  from a copy you already have.
- Every launcher update, server address and file list the launcher accepts is signed. It refuses anything that
  is not.

Dragon Soul belongs to its rights holders. This project is not affiliated with Giant Network or Neonga.

## Musician

Launcher 1.1.0 adds support for the Musician client update: choose Musician at
character creation and read its remaining skill and status descriptions in
English. The update keeps the original skill mechanics and game assets.

The launcher reconstructs the four updated files from the same verified original
downloads during installation and repair. See [Musician support](docs/musician.md)
for the versioned recipe and reproducible checks.

## Building

Windows, Rust (stable, MSVC), Node 22 and pnpm 10:

    ./scripts/fetch-innoextract.ps1     # the bundled unpacker (innoextract 1.9, zlib licence)
    cd launcher && pnpm install && pnpm tauri dev

## Layout

- `core/`: the install routine, recipes and signatures (no UI)
- `recipe-tool/`: builds, checks and applies recipes
- `launcher/`: the Tauri app
- `publish/`: signed server-info and recipes, deployed to GitHub Pages
