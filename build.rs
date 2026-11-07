// Rebuild when migrations change, so newly added SQL files get embedded
// (refinery::embed_migrations! reads ./migrations at compile time).
fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
