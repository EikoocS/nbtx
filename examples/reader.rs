use nbtx::{PlatformType, Reader};

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: cargo run --example reader -- <nbt-file>");

    let mut reader = Reader::try_new_with_path(&path, PlatformType::JavaEdition)
        .expect("failed to open NBT file");

    while reader.has_next() {
        let (nbt_path, value) = reader.next().expect("failed to read NBT entry");
        println!("{}: {:?}", nbt_path, value);
    }
}
