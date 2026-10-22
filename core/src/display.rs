//! Picks the game's resolution. The 2011 interface is fixed-pixel with a bitmap font, so on screens taller
//! than 1080 lines Auto picks the largest mode with the screen's aspect ratio and at most 1080 lines, and the
//! monitor scales it up (2560x1440 and 3840x2160 both get 1920x1080).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenMode {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ResolutionSetting {
    #[default]
    Auto,
    Native,
    Fixed {
        width: u32,
        height: u32,
    },
}

const MAX_AUTO_HEIGHT: u32 = 1080;

pub fn choose(setting: &ResolutionSetting, native: ScreenMode, modes: &[ScreenMode]) -> ScreenMode {
    match *setting {
        ResolutionSetting::Fixed { width, height } => ScreenMode { width, height },
        ResolutionSetting::Native => native,
        ResolutionSetting::Auto if native.height <= MAX_AUTO_HEIGHT => native,
        ResolutionSetting::Auto => {
            let aspect = f64::from(native.width) / f64::from(native.height);
            modes
                .iter()
                .copied()
                .filter(|m| {
                    m.height > 0
                        && m.height <= MAX_AUTO_HEIGHT
                        && (f64::from(m.width) / f64::from(m.height) - aspect).abs() < 0.01
                })
                .max_by_key(|m| (m.height, m.width))
                .unwrap_or(native)
        }
    }
}

/// The primary monitor's current mode and every 32-bit mode it offers, without duplicates.
#[cfg(windows)]
pub fn screen_modes() -> (ScreenMode, Vec<ScreenMode>) {
    use windows_sys::Win32::Graphics::Gdi::{
        EnumDisplaySettingsW, DEVMODEW, ENUM_CURRENT_SETTINGS,
    };
    let query = |index: u32| -> Option<DEVMODEW> {
        // SAFETY: DEVMODEW is plain data; dmSize tells the API which version of the struct we pass.
        let mut dm: DEVMODEW = unsafe { std::mem::zeroed() };
        dm.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
        (unsafe { EnumDisplaySettingsW(std::ptr::null(), index, &mut dm) } != 0).then_some(dm)
    };
    let native = query(ENUM_CURRENT_SETTINGS)
        .map(|dm| ScreenMode {
            width: dm.dmPelsWidth,
            height: dm.dmPelsHeight,
        })
        .unwrap_or(ScreenMode {
            width: 1920,
            height: 1080,
        });
    let mut modes = Vec::new();
    let mut i = 0;
    while let Some(dm) = query(i) {
        let m = ScreenMode {
            width: dm.dmPelsWidth,
            height: dm.dmPelsHeight,
        };
        if dm.dmBitsPerPel == 32 && !modes.contains(&m) {
            modes.push(m);
        }
        i += 1;
    }
    modes.sort_by_key(|m| (m.width, m.height));
    (native, modes)
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn m(width: u32, height: u32) -> ScreenMode {
        ScreenMode { width, height }
    }

    const MODES: &[ScreenMode] = &[
        m(1280, 720),
        m(1280, 1024),
        m(1600, 900),
        m(1920, 1080),
        m(2560, 1080),
        m(2560, 1440),
        m(3840, 2160),
    ];

    #[test]
    fn auto_follows_the_launch_script_rules() {
        let auto = ResolutionSetting::Auto;
        assert_eq!(choose(&auto, m(1920, 1080), MODES), m(1920, 1080));
        assert_eq!(choose(&auto, m(1366, 768), MODES), m(1366, 768));
        assert_eq!(choose(&auto, m(2560, 1440), MODES), m(1920, 1080));
        assert_eq!(choose(&auto, m(3840, 2160), MODES), m(1920, 1080));
        // 3440x1440 has no same-aspect mode at or below 1080 lines (2560x1080 is 0.02 off): stay native.
        assert_eq!(choose(&auto, m(3440, 1440), MODES), m(3440, 1440));
        assert_eq!(choose(&auto, m(2560, 1440), &[]), m(2560, 1440));
    }

    #[test]
    fn native_and_fixed_are_taken_as_they_are() {
        assert_eq!(
            choose(&ResolutionSetting::Native, m(2560, 1440), MODES),
            m(2560, 1440)
        );
        assert_eq!(
            choose(
                &ResolutionSetting::Fixed {
                    width: 1280,
                    height: 720
                },
                m(2560, 1440),
                MODES
            ),
            m(1280, 720)
        );
    }

    #[test]
    fn the_setting_serializes_with_a_kind_tag() {
        assert_eq!(
            serde_json::to_string(&ResolutionSetting::Auto).unwrap(),
            r#"{"kind":"auto"}"#
        );
        let fixed = ResolutionSetting::Fixed {
            width: 1280,
            height: 720,
        };
        assert_eq!(
            serde_json::to_string(&fixed).unwrap(),
            r#"{"kind":"fixed","width":1280,"height":720}"#
        );
    }
}
