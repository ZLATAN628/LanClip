use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = Path::new("proto/message.proto");
    tonic_build::compile_protos(path)?;
    Ok(())
}
