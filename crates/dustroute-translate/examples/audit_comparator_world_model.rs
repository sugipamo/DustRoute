//! Diagnostic replay of the retained comparator model, not live conformance.
#[path = "../tests/support/comparator_models.rs"]
mod comparator_models;

fn main() {
    for row in comparator_models::capture() {
        println!("{row}");
    }
}
