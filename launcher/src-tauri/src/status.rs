//! What the window shows. Sent to the UI through the `status` event and `get_status`.

use dsr_core::progress::Progress;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum UiStatus {
    Starting,
    UpdatingLauncher,
    NeedsFolder { suggested: String },
    Working { progress: Progress },
    Ready { offline: bool, note: Option<String> },
    Error { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_for_the_ui() {
        let s = UiStatus::Working {
            progress: Progress::Downloading {
                source: "installer".into(),
                done: 1,
                total: 2,
            },
        };
        assert_eq!(
            serde_json::to_string(&s).unwrap(),
            r#"{"kind":"working","progress":{"stage":"downloading","source":"installer","done":1,"total":2}}"#
        );
        assert_eq!(
            serde_json::to_string(&UiStatus::UpdatingLauncher).unwrap(),
            r#"{"kind":"updatingLauncher"}"#
        );
    }
}
