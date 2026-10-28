//! Offline check of a locally assembled, supported reference client.
//! Usage: cargo run -p dsr-core --example musician_check -- INPUT OUTPUT
use dsr_core::{aup, musician};
use std::{env, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err("expected reference and output directories".into());
    }
    let input = PathBuf::from(&args[0]);
    let output = PathBuf::from(&args[1]);
    for path in [
        "gameclient_release.exe",
        "data/skill/nskilldesc05.skill.aup",
        "data/nbuffdesc01.xml.aup",
        "data/createroletext.xml.aup",
    ] {
        let pack = musician::pack_for(path).unwrap();
        let original = fs::read(input.join(path))?;
        let result = musician::transform(pack, &original)?;
        if path.ends_with(".aup") {
            let raw = aup::decode(&original)?;
            assert_eq!(musician::transform(pack, &raw)?, result);
        }
        let dest = output.join(path);
        fs::create_dir_all(dest.parent().unwrap())?;
        fs::write(dest, &result)?;
        println!("{path}: fingerprint verified, {} bytes", result.len());
    }
    Ok(())
}
