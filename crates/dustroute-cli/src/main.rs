use std::collections::HashMap;
use std::env;
use std::error::Error;

use dustroute_core::{
    BaselineCompileConfig, BaselineCompiler, JavaExportConfig, LogicDag, compiled_circuit_datapack,
    decoder_1_to_2, full_adder, half_adder, half_subtractor, mux_2_to_1, semantics_datapack,
};

fn circuit(name: &str) -> Option<LogicDag> {
    match name {
        "half-adder" => Some(half_adder()),
        "half-subtractor" => Some(half_subtractor()),
        "mux2" => Some(mux_2_to_1()),
        "decoder1to2" => Some(decoder_1_to_2()),
        "full-adder" => Some(full_adder()),
        _ => None,
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let command = args.next().ok_or("usage: dustroute-cli eval|export ...")?;
    if command == "export-semantics" {
        let output = args.next().ok_or("missing output ZIP path")?;
        let namespace = args.next().unwrap_or_else(|| "ro_sem".into());
        let config = JavaExportConfig {
            namespace,
            ..JavaExportConfig::default()
        };
        let path = semantics_datapack(&config)?.write_zip(std::path::Path::new(&output))?;
        println!("{}", path.display());
        return Ok(());
    }
    let name = args.next().ok_or("missing circuit name")?;
    let dag = circuit(&name).ok_or("unknown circuit")?;
    if command == "export" {
        let output = args.next().ok_or("missing output ZIP path")?;
        let namespace = args.next().unwrap_or_else(|| "dustroute".into());
        let compiled = BaselineCompiler::new(BaselineCompileConfig::default()).compile(&dag)?;
        let config = JavaExportConfig {
            namespace,
            ..JavaExportConfig::default()
        };
        let pack = compiled_circuit_datapack(&compiled, &name.replace('-', "_"), &config)?;
        let path = pack.write_zip(std::path::Path::new(&output))?;
        println!("{}", path.display());
        return Ok(());
    }
    if command != "eval" {
        return Err(
            "usage: dustroute-cli eval CIRCUIT key=0|1 ... | export CIRCUIT OUTPUT.zip [namespace] | export-semantics OUTPUT.zip [namespace]"
                .into(),
        );
    }
    let mut inputs = HashMap::new();
    for assignment in args {
        let (key, value) = assignment.split_once('=').ok_or("expected key=0|1")?;
        let value = match value {
            "0" | "false" => false,
            "1" | "true" => true,
            _ => return Err(format!("invalid Boolean value: {value}").into()),
        };
        inputs.insert(key.to_owned(), value);
    }
    for (output, value) in dag.evaluate(&inputs)? {
        println!("{output}={}", u8::from(value));
    }
    Ok(())
}
