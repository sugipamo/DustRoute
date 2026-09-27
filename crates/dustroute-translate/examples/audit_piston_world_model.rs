#[path = "../tests/support/piston_models.rs"]
mod piston_models;

fn main() {
    for row in piston_models::capture() {
        println!("{row}");
    }
}
