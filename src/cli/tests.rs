use super::*;

fn sample_list_of_compounds() -> NbtValue {
    NbtValue::List {
        id: tag_id::COMPOUND,
        elements: vec![
            NbtValue::Compound(vec![
                ("id".to_string(), NbtValue::Int(123)),
                ("count".to_string(), NbtValue::Int(12)),
            ]),
            NbtValue::Compound(vec![
                ("id".to_string(), NbtValue::Int(123)),
                ("count".to_string(), NbtValue::Int(1000)),
            ]),
            NbtValue::Compound(vec![
                ("id".to_string(), NbtValue::Int(456)),
                ("count".to_string(), NbtValue::Int(1)),
            ]),
        ],
    }
}

fn sample_document() -> NbtValue {
    NbtValue::Compound(vec![("items".to_string(), sample_list_of_compounds())])
}

#[test]
fn regex_selector_matches_multiple_paths() {
    let doc = NbtValue::Compound(vec![(
        "root".to_string(),
        NbtValue::List {
            id: tag_id::COMPOUND,
            elements: vec![
                NbtValue::Compound(vec![("id".to_string(), NbtValue::Int(1))]),
                NbtValue::Compound(vec![("id".to_string(), NbtValue::Int(2))]),
            ],
        },
    )]);

    let selector = parse_path_selector(r"re:^root\[\d+\]\.id$").expect("selector should parse");
    let paths = resolve_selector_paths(&doc, &selector);
    assert_eq!(paths.len(), 2);
}

#[test]
fn where_expr_filters_list_compounds() {
    let clauses = parse_where_expr("id==123&&count<999").expect("where should parse");
    let mut list = sample_list_of_compounds();
    let NbtValue::List { elements, .. } = &mut list else {
        panic!("expected list");
    };

    elements.retain(|element| !where_matches_all(element, &clauses));

    assert_eq!(elements.len(), 2);
}

#[test]
fn where_regex_matches_string_field() {
    let clauses = parse_where_expr("name~=^foo.*").expect("where regex should parse");
    let element = NbtValue::Compound(vec![
        ("name".to_string(), NbtValue::String("foobar".to_string())),
        ("count".to_string(), NbtValue::Int(1)),
    ]);
    assert!(where_matches_all(&element, &clauses));
}

#[test]
fn where_ne_does_not_match_missing_field() {
    let clauses = parse_where_expr("missing!=1").expect("where should parse");
    let element = NbtValue::Compound(vec![("id".to_string(), NbtValue::Int(123))]);
    assert!(!where_matches_all(&element, &clauses));
}

#[test]
fn normalize_delete_paths_prefers_more_specific_match() {
    let broad = parse_path("items").expect("path should parse");
    let specific = parse_path("items[0].id").expect("path should parse");
    let normalized = normalize_delete_paths(vec![broad, specific.clone()]);
    assert_eq!(normalized, vec![specific]);
}

#[test]
fn where_delete_can_resolve_from_descendant_regex_matches() {
    let document = sample_document();
    let selector = parse_path_selector(r"re:^items\[\d+\]\.id$").expect("selector should parse");
    let matched_paths = resolve_selector_paths(&document, &selector);
    let list_targets = resolve_list_targets_for_where(&document, matched_paths);
    let expected = vec![parse_path("items").expect("path should parse")];
    assert_eq!(list_targets, expected);
}
