//! Progress events from the install routine; the launcher forwards them to the window.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "stage", rename_all = "camelCase")]
pub enum Progress {
    Checking {
        done: u64,
        total: u64,
    },
    Downloading {
        source: String,
        done: u64,
        total: u64,
    },
    Unpacking {
        source: String,
    },
    Building {
        done: u64,
        total: u64,
    },
    CleaningUp,
}
