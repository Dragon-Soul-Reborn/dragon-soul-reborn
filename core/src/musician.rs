//! Version-pinned native Musician menu and English text updates.
//! Only fingerprints and newly written English strings are shipped here.
use crate::{aup, hash, model::Pack};
use regex::bytes::Regex;
use serde::Deserialize;
use std::sync::OnceLock;

pub const MIN_LAUNCHER: &str = "1.1.0";

#[derive(Debug, thiserror::Error)]
pub enum MusicianError {
    #[error("unsupported Musician source fingerprint")]
    Unsupported,
    #[error("Musician source structure differs from the supported version")]
    Structure,
    #[error("Musician output fingerprint mismatch")]
    Output,
    #[error(transparent)]
    Aup(#[from] aup::AupError),
}

#[derive(Deserialize)]
struct Text {
    id: String,
    rank: String,
    text: String,
}

#[derive(Deserialize)]
struct Asset {
    kind: String,
    path: String,
    input_sha256: String,
    raw_sha256: String,
    output_sha256: String,
    output_size: usize,
}

#[derive(Deserialize)]
struct Data {
    skills: Vec<Text>,
    buffs: Vec<Text>,
    assets: Vec<Asset>,
}

fn data() -> &'static Data {
    static DATA: OnceLock<Data> = OnceLock::new();
    DATA.get_or_init(|| {
        serde_json::from_str(include_str!("musician.json")).expect("built-in Musician data")
    })
}

pub fn pack_for(path: &str) -> Option<Pack> {
    match crate::model::norm_path(path).as_str() {
        "gameclient_release.exe" => Some(Pack::MusicianMenu),
        "data/skill/nskilldesc05.skill.aup" => Some(Pack::MusicianSkills),
        "data/nbuffdesc01.xml.aup" => Some(Pack::MusicianBuffs),
        "data/createroletext.xml.aup" => Some(Pack::MusicianCreation),
        _ => None,
    }
}

pub fn is_musician(pack: Pack) -> bool {
    pack != Pack::Aup
}

pub fn transform(pack: Pack, input: &[u8]) -> Result<Vec<u8>, MusicianError> {
    let kind = match pack {
        Pack::MusicianMenu => "menu",
        Pack::MusicianSkills => "skills",
        Pack::MusicianBuffs => "buffs",
        Pack::MusicianCreation => "creation",
        Pack::Aup => return Err(MusicianError::Unsupported),
    };
    let asset = data()
        .assets
        .iter()
        .find(|a| a.kind == kind)
        .expect("built-in asset");
    debug_assert_eq!(pack_for(&asset.path), Some(pack));
    let digest = hash::sha256_bytes(input);
    let raw = if digest == asset.raw_sha256 {
        input.to_vec()
    } else if digest == asset.input_sha256 && pack != Pack::MusicianMenu {
        aup::decode(input)?
    } else {
        return Err(MusicianError::Unsupported);
    };
    let output = match pack {
        Pack::MusicianMenu => patch_menu(raw)?,
        Pack::MusicianSkills => aup::encode(&translate(raw, &data().skills, true)?),
        Pack::MusicianBuffs => aup::encode(&translate(raw, &data().buffs, false)?),
        Pack::MusicianCreation => {
            aup::encode(&replace_once(raw, b">Bard</FONT>", b">Musician</FONT>")?)
        }
        Pack::Aup => unreachable!(),
    };
    if output.len() != asset.output_size || hash::sha256_bytes(&output) != asset.output_sha256 {
        return Err(MusicianError::Output);
    }
    Ok(output)
}

fn patch_menu(mut raw: Vec<u8>) -> Result<Vec<u8>, MusicianError> {
    for (offset, instruction) in [(2_528_912, [0x83, 0xff, 4]), (2_529_742, [0x83, 0xf8, 4])] {
        if raw.get(offset..offset + 3) != Some(&instruction) {
            return Err(MusicianError::Structure);
        }
        raw[offset + 2] = 5;
    }
    Ok(raw)
}

fn replace_once(mut raw: Vec<u8>, old: &[u8], new: &[u8]) -> Result<Vec<u8>, MusicianError> {
    let positions: Vec<_> = raw
        .windows(old.len())
        .enumerate()
        .filter(|(_, b)| *b == old)
        .map(|(p, _)| p)
        .collect();
    if positions.len() != 1 {
        return Err(MusicianError::Structure);
    }
    raw.splice(positions[0]..positions[0] + old.len(), new.iter().copied());
    Ok(raw)
}

/// Byte regexes preserve the original GB18030 encoding, formatting and all
/// non-description attributes. Scope each replacement to one ID and one rank.
fn translate(mut raw: Vec<u8>, texts: &[Text], skills: bool) -> Result<Vec<u8>, MusicianError> {
    for text in texts {
        if !text.text.is_ascii()
            || !text.id.bytes().all(|c| c.is_ascii_digit())
            || !text.rank.bytes().all(|c| c.is_ascii_digit())
        {
            return Err(MusicianError::Structure);
        }
        let (tag, id_attr, close, level, desc) = if skills {
            (
                "SKILL_COMMON_SKILL",
                "SKILL_ID",
                "</LEVELSET>",
                "LEVEL",
                "LevelDesc",
            )
        } else {
            (r"[^/!?\s>]+", "Id", "</LevelSet>", "Level", "Description")
        };
        let outer = Regex::new(&format!(
            r#"(?-u)<{tag}\b[^>]*\b{id_attr}="{}"[^>]*>"#,
            text.id
        ))
        .expect("static regex");
        let starts: Vec<_> = outer.find_iter(&raw).collect();
        if starts.len() != 1 {
            return Err(MusicianError::Structure);
        }
        let start = starts[0].end();
        let end = raw[start..]
            .windows(close.len())
            .position(|b| b == close.as_bytes())
            .ok_or(MusicianError::Structure)?
            + start;
        let rank = Regex::new(&format!(
            r#"(?-u)<{level}\b[^>]*\bLevel="{}"[^>]*>"#,
            text.rank
        ))
        .expect("static regex");
        let ranks: Vec<_> = rank.find_iter(&raw[start..end]).collect();
        if ranks.len() != 1 {
            return Err(MusicianError::Structure);
        }
        let at = start + ranks[0].start();
        let attr = Regex::new(&format!(r#"(?-u)\b{desc}="([^"]*)""#)).expect("static regex");
        let captures = attr
            .captures(ranks[0].as_bytes())
            .ok_or(MusicianError::Structure)?;
        let value = captures.get(1).ok_or(MusicianError::Structure)?;
        let escaped = text
            .text
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;");
        raw.splice(at + value.start()..at + value.end(), escaped.bytes());
    }
    Ok(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translations_preserve_legacy_bytes_and_mechanics() {
        let raw = b"<SKILL_COMMON_SKILL SKILL_ID=\"42\"><LEVELSET><LEVEL Level=\"1\" Power=\"99\" LevelDesc=\"\x81\x40\"/><LEVEL Level=\"2\" LevelDesc=\"leave\"/></LEVELSET></SKILL_COMMON_SKILL>".to_vec();
        let text = Text {
            id: "42".into(),
            rank: "1".into(),
            text: "Heal & protect \"allies\".".into(),
        };
        let changed = translate(raw.clone(), &[text], true).unwrap();
        assert_eq!(
            changed,
            replace_once(raw, b"\x81\x40", b"Heal &amp; protect &quot;allies&quot;.").unwrap()
        );
    }

    #[test]
    fn rejects_missing_or_duplicate_rank() {
        let text = Text {
            id: "42".into(),
            rank: "1".into(),
            text: "Heal".into(),
        };
        for levels in [
            "",
            "<Level Level=\"1\" Description=\"x\"/><Level Level=\"1\" Description=\"x\"/>",
        ] {
            let raw = format!("<Buff Id=\"42\"><LevelSet>{levels}</LevelSet></Buff>").into_bytes();
            assert!(translate(raw, std::slice::from_ref(&text), false).is_err());
        }
    }

    #[test]
    fn rejects_unknown_or_already_modified_inputs() {
        for pack in [
            Pack::MusicianMenu,
            Pack::MusicianSkills,
            Pack::MusicianBuffs,
            Pack::MusicianCreation,
        ] {
            assert!(matches!(
                transform(pack, b"synthetic unsupported input"),
                Err(MusicianError::Unsupported)
            ));
        }
    }
}
