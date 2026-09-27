//! Compare existing model scopes without granting live conformance evidence.
#[path = "../tests/support/repeater_models.rs"]
mod repeater_models;

fn main() {
    for row in repeater_models::capture() {
        println!("{row}");
    }
}
