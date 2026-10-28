# Native Musician support

The supported client contains the Musician class and its assets. Two class-menu
loop bounds hide its fifth entry. The Musician recipe exposes that entry and
translates 50 skill-rank descriptions and 48 status-effect tooltips. The class
description heading uses the same Musician name as the rest of the game.

The transforms accept only the supported source fingerprints and verify the
complete output fingerprint. Skill mechanics, numeric combat fields, meshes and
textures remain unchanged. Original game files are reconstructed locally from
the existing archive sources; they are not committed or redistributed here.

Build a **new recipe version** with `recipe-tool build --musician`, using
`--min-launcher 1.1.0` or later. The original recipe builder and previous recipes
remain unchanged. The four transforms are restricted to their exact file paths
and exact verification mode. An older launcher cannot parse these transforms;
publish them only with a launcher version that supports them.

Recipe `0.27-r3` requires launcher 1.1.0 and updates exactly those four files.
The original `0.27-r1` recipe remains available. Publish the compatible launcher
before switching the signed server feed to `0.27-r3`, so existing installations
can update their launcher first.

The server must also accept native occupation 4. A menu change alone is
insufficient. Server compatibility is maintained separately from this launcher.

For offline verification against an already assembled supported reference client:

```text
cargo run -p dsr-core --example musician_check -- REFERENCE_FOLDER OUTPUT_FOLDER
```

This checks both packed and raw XML inputs, writes the four resulting files to a
separate output folder and verifies their fingerprints. All committed automated
test fixtures are synthetic.
