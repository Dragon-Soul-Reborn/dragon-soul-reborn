//! Writes the display settings into the game's config files before every start, as the original launcher
//! did. Only these keys change; everything else the player set in the game stays. VerticalSync is always on:
//! the 2011 engine times movement with millisecond timers and runs far too fast uncapped.

use regex::bytes::{NoExpand, Regex};
use std::{fs, io, path::Path};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayChoice {
    pub width: u32,
    pub height: u32,
    pub fullscreen: bool,
}

pub fn patch_engine_config(src: &[u8], d: &DisplayChoice) -> Vec<u8> {
    let windowed = Regex::new(r#"windowed="[^"]*""#).expect("valid regex");
    let resolution =
        Regex::new(r#"<Resolution width="[^"]*" height="[^"]*""#).expect("valid regex");
    let mode = format!(r#"windowed="{}""#, u8::from(!d.fullscreen));
    let size = format!(r#"<Resolution width="{}" height="{}""#, d.width, d.height);
    let s = windowed.replace_all(src, NoExpand(mode.as_bytes()));
    resolution
        .replace_all(&s, NoExpand(size.as_bytes()))
        .into_owned()
}

pub fn patch_localsetup(src: &[u8], d: &DisplayChoice) -> Vec<u8> {
    let keys = [
        ("FullScreen", u32::from(d.fullscreen)),
        ("Width", d.width),
        ("Height", d.height),
        ("BitsPerPel", 32),
        ("Frequency", 0),
        ("VerticalSync", 1),
    ];
    let mut out = src.to_vec();
    for (key, value) in keys {
        let re = Regex::new(&format!(r#"<{key} parameter="[^"]*" />"#)).expect("valid regex");
        let line = format!(r#"<{key} parameter="{value}" />"#);
        out = re.replace_all(&out, NoExpand(line.as_bytes())).into_owned();
    }
    out
}

pub fn apply_display(folder: &Path, d: &DisplayChoice) -> io::Result<()> {
    let engine = folder.join("config").join("engine_config.xml");
    fs::write(&engine, patch_engine_config(&fs::read(&engine)?, d))?;
    let local = folder.join("config").join("localsetup.xml");
    fs::write(&local, patch_localsetup(&fs::read(&local)?, d))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENGINE: &str = r#"<Root><Graphics><Mode windowed="%windowmode%" /><Resolution width="%screenwidth%" height="%screenheight%" /></Graphics></Root>"#;
    const LOCAL: &str = r#"<LocalSetup><FullScreen parameter="1" /><Width parameter="800" /><Height parameter="600" /><BitsPerPel parameter="16" /><Frequency parameter="60" /><Sound parameter="1" /><VerticalSync parameter="0" /></LocalSetup>"#;
    const FULL: DisplayChoice = DisplayChoice {
        width: 1920,
        height: 1080,
        fullscreen: true,
    };

    fn text(b: Vec<u8>) -> String {
        String::from_utf8(b).unwrap()
    }

    #[test]
    fn engine_config_gets_the_mode_and_resolution() {
        let out = text(patch_engine_config(ENGINE.as_bytes(), &FULL));
        assert!(
            out.contains(r#"windowed="0""#)
                && out.contains(r#"<Resolution width="1920" height="1080""#)
        );
        let windowed = text(patch_engine_config(
            ENGINE.as_bytes(),
            &DisplayChoice {
                fullscreen: false,
                ..FULL
            },
        ));
        assert!(windowed.contains(r#"windowed="1""#));
    }

    #[test]
    fn localsetup_gets_display_keys_and_vsync_and_keeps_the_rest() {
        let out = text(patch_localsetup(LOCAL.as_bytes(), &FULL));
        for want in [
            r#"<FullScreen parameter="1" />"#,
            r#"<Width parameter="1920" />"#,
            r#"<Height parameter="1080" />"#,
            r#"<BitsPerPel parameter="32" />"#,
            r#"<Frequency parameter="0" />"#,
            r#"<VerticalSync parameter="1" />"#,
            r#"<Sound parameter="1" />"#,
        ] {
            assert!(out.contains(want), "{want}");
        }
    }

    #[test]
    fn apply_display_rewrites_both_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("config")).unwrap();
        fs::write(dir.path().join("config/engine_config.xml"), ENGINE).unwrap();
        fs::write(dir.path().join("config/localsetup.xml"), LOCAL).unwrap();
        apply_display(dir.path(), &FULL).unwrap();
        assert!(
            fs::read_to_string(dir.path().join("config/engine_config.xml"))
                .unwrap()
                .contains(r#"width="1920""#)
        );
        assert!(fs::read_to_string(dir.path().join("config/localsetup.xml"))
            .unwrap()
            .contains(r#"<VerticalSync parameter="1" />"#));
    }
}
