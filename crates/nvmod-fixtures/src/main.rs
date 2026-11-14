// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod-fixtures <folder> [ok|wrong-checksum|failing-installer]`: write the synthetic fixtures of a setup run into
//! the (empty or new) folder and print one JSON document with the arguments to run, to stdout.

#[cfg(unix)]
fn main() {
    use nvmod_fixtures::{Variant, write_rig};
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(dir), variant) = (args.first(), args.get(1).map_or("ok", String::as_str)) else {
        eprintln!("usage: nvmod-fixtures <folder> [ok|wrong-checksum|failing-installer]");
        std::process::exit(5);
    };
    let Some(variant) = Variant::parse(variant) else {
        eprintln!("unknown variant {variant}");
        std::process::exit(5);
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).expect("create the folder");
    let rig = write_rig(&dir, variant);
    let doc = serde_json::json!({
        "base": rig.base,
        "instance": rig.instance,
        "setup_args": rig.setup_args,
        "verify_args": rig.verify_args,
        "expected_setup_exit": rig.expected_setup_exit,
    });
    println!("{doc}");
}

#[cfg(not(unix))]
fn main() {
    eprintln!("nvmod-fixtures needs a unix host");
    std::process::exit(5);
}
