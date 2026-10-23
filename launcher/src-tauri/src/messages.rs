//! Plain-language messages for everything that can go wrong.

use dsr_core::{
    apply::ApplyError, extract::ExtractError, feed::FeedError, fetch::FetchError,
    routine::RoutineError,
};

pub fn routine_message(e: &RoutineError) -> String {
    match e {
        RoutineError::Model(e) => format!("The game file list is unsafe or invalid: {e}"),
        RoutineError::GameRunning => "Close Dragon Soul first, then press Retry.".into(),
        RoutineError::NoSpace { needed, available } => format!(
            "Not enough free space: {} needed, {} free. Free up space, or pick another folder in Settings.",
            gb(*needed),
            gb(*available)
        ),
        RoutineError::NotWritable(p) => format!("The launcher can't write to {}. Pick another folder in Settings.", p.display()),
        RoutineError::Fetch(FetchError::Cancelled) | RoutineError::Apply(ApplyError::Cancelled) => {
            "Stopped. Press Retry to continue where it left off.".into()
        }
        RoutineError::Fetch(FetchError::Unreachable { .. }) => {
            "archive.org isn't answering. Try again later, or point the launcher at an installer copy you already have.".into()
        }
        RoutineError::Fetch(FetchError::Mismatch) => "A download from archive.org came out damaged twice. Try again later.".into(),
        RoutineError::Fetch(FetchError::NotAllowed(url)) => {
            format!("The update asked for a download from an address the launcher doesn't trust ({url}). Nothing was downloaded.")
        }
        RoutineError::Fetch(FetchError::Io(e)) | RoutineError::Apply(ApplyError::Io(e)) | RoutineError::Io(e) => {
            format!("A file couldn't be written ({e}). If this keeps happening, your antivirus may be blocking the game folder.")
        }
        RoutineError::Apply(ApplyError::Extract(ExtractError::Spawn(_))) => "The launcher's unpacker is missing. Reinstall the launcher.".into(),
        RoutineError::Apply(ApplyError::Extract(e)) => format!("Unpacking failed: {e}"),
        RoutineError::Apply(ApplyError::BuiltMismatch(p)) => format!("{p} came out different from what this game version expects. Please report this."),
    }
}

pub fn feed_message(e: &FeedError) -> String {
    match e {
        FeedError::Unreachable => {
            "Can't reach the update server, and there's no saved copy yet. Check your internet connection and press Retry.".into()
        }
        FeedError::BadSignature => "The update server's files failed their safety check, so the launcher won't use them. Try again later.".into(),
        FeedError::Invalid(detail) => format!("The update server sent something this launcher can't read ({detail}). Restart the launcher to update it."),
    }
}

fn gb(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / 1e9)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explains_space_and_offline_problems() {
        let space = routine_message(&RoutineError::NoSpace {
            needed: 1_450_000_000,
            available: 300_000_000,
        });
        assert!(space.contains("1.4 GB needed") && space.contains("0.3 GB free"));
        let offline = routine_message(&RoutineError::Fetch(FetchError::Unreachable {
            url: "u".into(),
            reason: "r".into(),
        }));
        assert!(offline.contains("archive.org isn't answering"));
        assert!(feed_message(&FeedError::Unreachable).contains("Retry"));
    }
}
