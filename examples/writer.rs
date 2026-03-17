use nbtx::{NbtComponent, PlatformType, RootType, Writer};

fn main() {
    let sink: Vec<u8> = Vec::new();
    let mut writer = Writer::try_new(
        Box::new(std::io::Cursor::new(sink)),
        PlatformType::JavaEdition,
        RootType::Compound,
    )
    .expect("failed to initialize writer");

    writer
        .write("Name", NbtComponent::String("Notch".to_string()))
        .expect("failed to write Name");
    writer
        .write("Score", NbtComponent::Int(42))
        .expect("failed to write Score");
    writer.end().expect("failed to end root compound");
    writer.finish().expect("failed to finalize document");

    println!("writer example completed");
}
