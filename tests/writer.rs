use nbtx::{NbtComponent, PlatformType, RootType, Writer};
use std::cell::RefCell;
use std::io::Write;
use std::rc::Rc;

struct SharedBuffer {
    inner: Rc<RefCell<Vec<u8>>>,
}

impl Write for SharedBuffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.inner.borrow_mut().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn nbt_writer() {
    let expected: [u8; _] = [
        // **HEAD**
        0x0a, 0x00, 0x00, // Int SpawnX : 23456
        0x03, 0x00, 0x06, 0x53, 0x70, 0x61, 0x77, 0x6E, 0x58, 0x00, 0x00, 0x5b, 0xa0,
        // String Name : Notch
        0x08, 0x00, 0x04, 0x4e, 0x61, 0x6d, 0x65, 0x00, 0x05, 0x4e, 0x6f, 0x74, 0x63, 0x68,
        // Float An_Example : 1.325
        0x05, 0x00, 0x0a, 0x41, 0x6E, 0x5F, 0x45, 0x78, 0x61, 0x6D, 0x70, 0x6C, 0x65, 0x3f, 0xa9,
        0x99, 0x9a, // Int_Array Chunk [ 384 , 754 ]
        0x0b, 0x00, 0x05, 0x43, 0x68, 0x75, 0x6E, 0x6B, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x01,
        0x80, 0x00, 0x00, 0x02, 0xf2, // List NameList [Steve, Alex, Noob]
        0x09, 0x00, 0x08, 0x4e, 0x61, 0x6d, 0x65, 0x4c, 0x69, 0x73, 0x74, 0x08, 0x00, 0x00, 0x00,
        0x03, 0x00, 0x05, 0x53, 0x74, 0x65, 0x76, 0x65, 0x00, 0x04, 0x41, 0x6c, 0x65, 0x78, 0x00,
        0x04, 0x4e, 0x6f, 0x6f, 0x62, // Compound **HEAD**  Rank
        0x0a, 0x00, 0x04, 0x52, 0x61, 0x6E, 0x6B, // String Winner : Steve
        0x08, 0x00, 0x06, 0x57, 0x69, 0x6E, 0x6E, 0x65, 0x72, 0x00, 0x05, 0x53, 0x74, 0x65, 0x76,
        0x65, // Short Count : 31
        0x02, 0x00, 0x05, 0x43, 0x6F, 0x75, 0x6E, 0x74, 0x00, 0x1f, // **END**
        0x00, // **END
        0x00,
    ];

    let output = Rc::new(RefCell::new(Vec::new()));
    let sink = SharedBuffer {
        inner: Rc::clone(&output),
    };

    let mut writer = Writer::new(
        Box::new(sink),
        PlatformType::JavaEdition,
        RootType::Compound,
    );

    writer
        .write("SpawnX", NbtComponent::Int(23456))
        .expect("failed to write SpawnX");
    writer
        .write("Name", NbtComponent::String("Notch".into()))
        .expect("failed to write Name");
    writer
        .write("An_Example", NbtComponent::Float(1.325))
        .expect("failed to write An_Example");
    writer
        .write("Chunk", NbtComponent::IntArray(vec![384, 754]))
        .expect("failed to write Chunk");

    writer
        .write(
            "NameList",
            NbtComponent::List {
                id: 0x08,
                length: 3,
            },
        )
        .expect("failed to start NameList");
    writer
        .write("", NbtComponent::String("Steve".into()))
        .expect("failed to write NameList[0]");
    writer
        .write("", NbtComponent::String("Alex".into()))
        .expect("failed to write NameList[1]");
    writer
        .write("", NbtComponent::String("Noob".into()))
        .expect("failed to write NameList[2]");

    writer
        .write("Rank", NbtComponent::Compound)
        .expect("failed to start Rank");
    writer
        .write("Winner", NbtComponent::String("Steve".into()))
        .expect("failed to write Rank.Winner");
    writer
        .write("Count", NbtComponent::Short(31))
        .expect("failed to write Rank.Count");
    writer.end().expect("failed to end Rank");
    writer.end().expect("failed to end root");
    writer.finish().expect("failed to finish writer");

    let actual = output.borrow().clone();
    assert_eq!(actual, expected);
}
