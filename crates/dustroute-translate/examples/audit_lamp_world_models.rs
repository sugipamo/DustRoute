#[path = "../tests/support/lamp_models.rs"]
mod lamp_models;

fn main() {
    for row in lamp_models::capture() {
        println!("{row}");
    }
}
