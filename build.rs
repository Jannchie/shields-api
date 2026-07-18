fn main() {
    // Cargo exposes no env var for dependency versions, so read the shields
    // version out of Cargo.lock at build time.
    let lock = std::fs::read_to_string("Cargo.lock").unwrap_or_default();
    let mut version = "unknown";
    let mut lines = lock.lines();
    while let Some(line) = lines.next() {
        if line.trim() == "name = \"shields\"" {
            if let Some(v) = lines
                .next()
                .and_then(|l| l.trim().strip_prefix("version = \""))
            {
                version = v.trim_end_matches('"');
            }
            break;
        }
    }
    println!("cargo:rustc-env=SHIELDS_CRATE_VERSION={version}");
    println!("cargo:rerun-if-changed=Cargo.lock");
}
