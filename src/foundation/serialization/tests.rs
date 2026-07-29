use super::parser::MAX_JSON_NESTING_DEPTH;
use super::*;

#[test]
fn escapes_json_string_content_without_adding_quotes() {
    assert_eq!(
        escape_string_content("한글\n\"quoted\"\\path\u{0008}"),
        "한글\\n\\\"quoted\\\"\\\\path\\u0008"
    );
}

#[test]
fn rejects_duplicate_unknown_escape_type_and_trailing() {
    for input in [
        r#"{"a":"x","a":"y"}"#,
        r#"{"b":"x"}"#,
        r#"{"a":"\q"}"#,
        r#"{"a":1}"#,
        r#"{"a":"x"} garbage"#,
    ] {
        let parsed = parse_object(input, &["a"], "fixture");
        if let Ok(object) = parsed {
            assert!(string(&object, "a", "fixture").is_err());
        }
    }
}

#[test]
fn rejects_leading_zero_number() {
    assert!(parse_object("{\"schema\":01}", &["schema"], "fixture").is_err());
}

#[test]
fn generic_parser_accepts_standard_numbers_and_surrogate_pairs() {
    let parsed = parse_value(
        r#"{"negative":-1,"fraction":1.25,"exponent":2e3,"emoji":"\uD83D\uDE00"}"#,
        "fixture",
    )
    .unwrap();
    let Value::Object(object) = parsed else {
        panic!("object가 필요합니다.");
    };

    assert_eq!(object.get("negative"), Some(&Value::Decimal("-1".into())));
    assert_eq!(object.get("fraction"), Some(&Value::Decimal("1.25".into())));
    assert_eq!(object.get("exponent"), Some(&Value::Decimal("2e3".into())));
    assert_eq!(object.get("emoji"), Some(&Value::String("😀".into())));
}

#[test]
fn ordered_object_round_trips_compact_bytes_and_checks_exact_order() {
    let input = r#"{"z":340282366920938463463374607431768211455,"a":[true,"한글\n",null]}"#;
    let value = parse_value(input, "ordered fixture").unwrap();

    assert_eq!(render_compact(&value), input);
    assert!(parse_object_exact_order(input, &["z", "a"], "ordered fixture").is_ok());
    assert!(parse_object_exact_order(input, &["a", "z"], "ordered fixture").is_err());
}

#[test]
fn checked_u128_and_u64_reject_narrowing_overflow() {
    let input = r#"{"small":18446744073709551615,"large":18446744073709551616}"#;
    let object = parse_object_exact_order(input, &["small", "large"], "number fixture").unwrap();

    assert_eq!(
        number(&object, "small", "number fixture").unwrap(),
        u64::MAX
    );
    assert_eq!(
        number_u128(&object, "large", "number fixture").unwrap(),
        u64::MAX as u128 + 1
    );
    assert!(number(&object, "large", "number fixture").is_err());
}

#[test]
fn canonical_object_rejects_noncanonical_bytes_and_numeric_spellings() {
    for input in [
        "{\"n\": 1}",
        " {\"n\":1}",
        "{\"n\":1}\n",
        "{\"n\":-1}",
        "{\"n\":1.0}",
        "{\"n\":1e0}",
        "{\"n\":\"\\u0061\"}",
        "{\"n\":1,\"extra\":2}",
    ] {
        assert!(parse_canonical_object(input, &["n"], "canonical fixture").is_err());
    }
}

#[test]
fn canonical_unsigned_boundaries_round_trip_byte_exactly() {
    let input = r#"{"zero":0,"u64":18446744073709551615,"u128":340282366920938463463374607431768211455,"nested":{"value":7},"array":[0,true,null,"한글"]}"#;
    let object = parse_canonical_object(
        input,
        &["zero", "u64", "u128", "nested", "array"],
        "canonical fixture",
    )
    .unwrap();

    assert_eq!(
        canonical_u64(&object, "zero", "canonical fixture").unwrap(),
        0
    );
    assert_eq!(
        canonical_u64(&object, "u64", "canonical fixture").unwrap(),
        u64::MAX
    );
    assert_eq!(
        canonical_u128(&object, "u128", "canonical fixture").unwrap(),
        u128::MAX
    );
    assert_eq!(render_canonical_object(&object), input);
    assert!(parse_canonical_object(
        r#"{"n":340282366920938463463374607431768211456}"#,
        &["n"],
        "overflow fixture"
    )
    .is_err());
}

#[test]
fn nesting_depth_is_bounded_before_recursive_descent() {
    let at_limit = format!(
        "{}0{}",
        "[".repeat(MAX_JSON_NESTING_DEPTH),
        "]".repeat(MAX_JSON_NESTING_DEPTH)
    );
    let beyond_limit = format!(
        "{}0{}",
        "[".repeat(MAX_JSON_NESTING_DEPTH + 1),
        "]".repeat(MAX_JSON_NESTING_DEPTH + 1)
    );

    assert!(parse_value(&at_limit, "depth fixture").is_ok());
    let error = parse_value(&beyond_limit, "depth fixture").unwrap_err();
    assert!(error.message.contains("nesting depth budget exceeded"));
}
