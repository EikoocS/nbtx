use nbtx::{NbtComponent, PlatformType, Reader, RootType, Writer, tag_id};
use std::collections::BTreeMap;
use std::fs::File;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_temp_file(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be valid")
        .as_nanos();
    path.push(format!("nbtx_{name}_{nanos}.nbt"));
    path
}

fn write_sample_file(path: &PathBuf) {
    let file = File::create(path).expect("failed to create sample nbt file");
    let mut writer = Writer::new(
        Box::new(file),
        PlatformType::JavaEdition,
        RootType::Compound,
    );

    writer
        .write(
            "items",
            NbtComponent::List {
                id: tag_id::COMPOUND,
                length: 3,
            },
        )
        .expect("failed to write items list");

    writer
        .write("", NbtComponent::Compound)
        .expect("failed to start item[0]");
    writer
        .write("id", NbtComponent::Int(123))
        .expect("failed to write item[0].id");
    writer
        .write("count", NbtComponent::Int(12))
        .expect("failed to write item[0].count");
    writer.end().expect("failed to end item[0]");

    writer
        .write("", NbtComponent::Compound)
        .expect("failed to start item[1]");
    writer
        .write("id", NbtComponent::Int(123))
        .expect("failed to write item[1].id");
    writer
        .write("count", NbtComponent::Int(1000))
        .expect("failed to write item[1].count");
    writer.end().expect("failed to end item[1]");

    writer
        .write("", NbtComponent::Compound)
        .expect("failed to start item[2]");
    writer
        .write("id", NbtComponent::Int(456))
        .expect("failed to write item[2].id");
    writer
        .write("count", NbtComponent::Int(1))
        .expect("failed to write item[2].count");
    writer.end().expect("failed to end item[2]");

    writer.end().expect("failed to end root");
    writer.finish().expect("failed to finish writer");
}

fn read_item_pairs(path: &PathBuf) -> Vec<(i32, i32)> {
    let mut reader = Reader::try_new_with_path(
        path.to_str().expect("temp path should be valid utf8"),
        PlatformType::JavaEdition,
    )
    .expect("failed to read nbt file");

    let mut items: BTreeMap<usize, (Option<i32>, Option<i32>)> = BTreeMap::new();
    while reader.has_next() {
        let (entry_path, component) = reader.next().expect("failed to read nbt entry");

        if !entry_path.starts_with("items[") {
            continue;
        }

        let Some(close_index) = entry_path.find(']') else {
            continue;
        };
        let index: usize = entry_path[6..close_index]
            .parse()
            .expect("list index should parse");

        if entry_path.ends_with(".id") {
            if let NbtComponent::Int(id) = component {
                items.entry(index).or_insert((None, None)).0 = Some(id);
            }
        }
        if entry_path.ends_with(".count") {
            if let NbtComponent::Int(count) = component {
                items.entry(index).or_insert((None, None)).1 = Some(count);
            }
        }
    }

    items
        .into_values()
        .map(|(id, count)| {
            (
                id.expect("item id should exist"),
                count.expect("item count should exist"),
            )
        })
        .collect()
}

fn run_cli(args: &[&str]) {
    let output = Command::new(env!("CARGO_BIN_EXE_nbtx"))
        .args(args)
        .output()
        .expect("failed to run nbtx command");

    assert!(
        output.status.success(),
        "nbtx command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn delete_where_keeps_list_and_removes_matching_compounds() {
    let path = unique_temp_file("delete_where_exact");
    write_sample_file(&path);

    run_cli(&[
        path.to_str().expect("temp path should be valid utf8"),
        "items",
        "--delete",
        "--where",
        "id==123&&count<999",
    ]);

    let pairs = read_item_pairs(&path);
    assert_eq!(pairs.len(), 2);
    assert!(pairs.contains(&(123, 1000)));
    assert!(pairs.contains(&(456, 1)));

    let _ = std::fs::remove_file(path);
}

#[test]
fn delete_where_can_target_list_via_descendant_regex() {
    let path = unique_temp_file("delete_where_regex");
    write_sample_file(&path);

    run_cli(&[
        path.to_str().expect("temp path should be valid utf8"),
        r"re:^items\[\d+\]\.id$",
        "--delete",
        "--where",
        "id==123&&count<999",
    ]);

    let pairs = read_item_pairs(&path);
    assert_eq!(pairs.len(), 2);
    assert!(pairs.contains(&(123, 1000)));
    assert!(pairs.contains(&(456, 1)));

    let _ = std::fs::remove_file(path);
}

#[test]
fn delete_where_on_list_path_does_not_remove_list_container() {
    let path = unique_temp_file("delete_where_list_path");
    write_sample_file(&path);

    run_cli(&[
        path.to_str().expect("temp path should be valid utf8"),
        r"re:^items$",
        "--delete",
        "--where",
        "id==123&&count<999",
    ]);

    let pairs = read_item_pairs(&path);
    assert_eq!(pairs.len(), 2);
    assert!(pairs.contains(&(123, 1000)));
    assert!(pairs.contains(&(456, 1)));

    let _ = std::fs::remove_file(path);
}
