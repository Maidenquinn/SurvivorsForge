//! `forge-scan <zip> [--folders A,B] [--foreign C,D]`: scans a local zip and prints the report as JSON.
//! Exit code 0 when clean, 1 when flagged, 2 on usage or file errors.

use std::process::ExitCode;

fn list(value: Option<String>) -> Vec<String> {
    value.unwrap_or_default().split(',').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect()
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let (mut zip, mut folders, mut foreign) = (None, None, None);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--folders" => folders = args.next(),
            "--foreign" => foreign = args.next(),
            _ if zip.is_none() && !arg.starts_with("--") => zip = Some(arg),
            _ => {
                eprintln!("usage: forge-scan <zip> [--folders A,B] [--foreign C,D]");
                return ExitCode::from(2);
            }
        }
    }
    let Some(zip) = zip else {
        eprintln!("usage: forge-scan <zip> [--folders A,B] [--foreign C,D]");
        return ExitCode::from(2);
    };
    let bytes = match std::fs::read(&zip) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("cannot read {zip}: {e}");
            return ExitCode::from(2);
        }
    };
    let report = forge_scan::scan(&bytes, &list(folders), &list(foreign));
    println!("{}", serde_json::to_string_pretty(&report).expect("report serializes"));
    if report.flagged() { ExitCode::from(1) } else { ExitCode::SUCCESS }
}
