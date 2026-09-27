//! Run explicitly to reproduce built-in assets; never part of runtime loading.
use dustroute_translate::blueprint_generation::generate_builtin_blueprints;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let generated = generate_builtin_blueprints()?;
    let output = generated.to_json()? + "\n";
    match std::env::args().nth(1).as_deref() {
        Some("--check") => {
            if output != dustroute_library::builtin_blueprints::BUILTIN_BLUEPRINT_ARCHIVE {
                return Err("generated blueprints differ from pinned revisions; review changes and assign new revisions before replacing established definitions".into());
            }
            println!("Built-in blueprint archive matches the independent authoring recipes.");
        }
        Some(path) => {
            match std::fs::read_to_string(path) {
                Ok(existing) => {
                    let previous =
                        dustroute_library::blueprint::BlueprintCatalog::from_json(&existing)?;
                    if previous
                        .revisions()
                        .any(|record| generated.revision(&record.id) != Some(record))
                        || previous
                            .classifications()
                            .any(|record| generated.classification(&record.id) != Some(record))
                        || previous
                            .type_revisions()
                            .any(|record| generated.type_revision(&record.id) != Some(record))
                        || previous
                            .assemblies()
                            .any(|record| generated.assembly(&record.id) != Some(record))
                    {
                        return Err("refusing to rebind or remove an existing revision; preserve old definitions and assign new IDs".into());
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            std::fs::write(path, output)?;
        }
        None => print!("{output}"),
    }
    Ok(())
}
