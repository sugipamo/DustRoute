#[path = "../tests/support/observer_models.rs"]
mod observer_models;

fn main() {
    for row in observer_models::capture() {
        println!("{row}");
    }
}
