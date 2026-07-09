use super::*;
use serde::Deserialize;
use serde_json::json;

#[test]
fn select_keeps_requested_fields_and_fills_missing_ones() {
    let value = json!({
        "name": "Ada",
        "age": 37,
        "ignored": true
    });

    let selected = value.select(["name", "date", "age"]).unwrap();

    assert_eq!(
        selected,
        json!({
            "name": "Ada",
            "date": null,
            "age": 37
        })
    );
}

#[test]
fn select_treats_non_objects_as_empty_objects() {
    let selected = json!(42).select(["name"]).unwrap();

    assert_eq!(selected, json!({ "name": null }));
}

#[test]
fn add_accepts_serializable_values() {
    #[derive(Serialize)]
    struct Site {
        name: String,
        active: bool,
    }

    let value = json!({ "name": "Ada" })
        .add(
            "site",
            Site {
                name: "corporate".to_owned(),
                active: true,
            },
        )
        .unwrap();

    assert_eq!(
        value,
        json!({
            "name": "Ada",
            "site": {
                "name": "corporate",
                "active": true
            }
        })
    );
}

#[test]
fn add_treats_non_objects_as_empty_objects() {
    let value = json!(42).add("site", "corporate").unwrap();

    assert_eq!(value, json!({ "site": "corporate" }));
}

#[test]
fn merge_adds_fields_without_overriding_existing_fields() {
    let value = json!({
        "name": "Ada",
        "site": "original"
    })
    .merge(json!({
        "site": "corporate",
        "age": 37
    }))
    .unwrap();

    assert_eq!(
        value,
        json!({
            "name": "Ada",
            "site": "original",
            "age": 37
        })
    );
}

#[test]
fn merge_treats_non_object_inputs_as_empty_objects() {
    let value = json!(42).merge(json!({ "site": "corporate" })).unwrap();

    assert_eq!(value, json!({ "site": "corporate" }));
}

#[test]
fn merge_ignores_non_object_fields() {
    let value = json!({ "name": "Ada" }).merge(json!(42)).unwrap();

    assert_eq!(value, json!({ "name": "Ada" }));
}

#[test]
fn merge_eval_calculates_and_merges_fields_without_overriding_existing_fields() {
    let value = json!({
        "first_name": "Ada",
        "last_name": "Lovelace",
        "display_name": "existing"
    })
    .merge_eval(|value| {
        json!({
            "display_name": format_value(value, "{first_name} {last_name}"),
            "slug": format_value(value, "{first_name}-{last_name}").to_lowercase()
        })
    })
    .unwrap();

    assert_eq!(
        value,
        json!({
            "first_name": "Ada",
            "last_name": "Lovelace",
            "display_name": "existing",
            "slug": "ada-lovelace"
        })
    );
}

#[test]
fn parse_text_parses_string_fields_and_merges_without_overriding_existing_fields() {
    let value = json!({
        "raw": "Ada Lovelace (research)",
        "name": "existing"
    })
    .parse_text("raw", "{name} ({/metadata/department})")
    .unwrap();

    assert_eq!(
        value,
        json!({
            "raw": "Ada Lovelace (research)",
            "name": "existing",
            "metadata": {
                "department": "research"
            }
        })
    );
}

#[test]
fn parse_text_supports_json_pointer_source_fields() {
    let value = json!({
        "input": {
            "raw": "Ada <ada@example.test>"
        }
    })
    .parse_text("/input/raw", "{name} <{email}>")
    .unwrap();

    assert_eq!(
        value,
        json!({
            "input": {
                "raw": "Ada <ada@example.test>"
            },
            "name": "Ada",
            "email": "ada@example.test"
        })
    );
}

#[test]
fn parse_text_works_in_result_pipelines() {
    let value = serde_json::from_str::<Value>(r#"{"raw":"Ada <ada@example.test>"}"#)
        .parse_text("raw", "{name} <{email}>")
        .unwrap();

    assert_eq!(
        value,
        json!({
            "raw": "Ada <ada@example.test>",
            "name": "Ada",
            "email": "ada@example.test"
        })
    );
}

#[test]
fn parsed_results_can_be_transformed_without_intermediate_question_marks() {
    let value = serde_json::from_str::<Value>(r#"{"name":"Ada","age":37}"#)
        .require_shape(json!({ "name": "", "age": 0 }))
        .select(["name", "date", "age"])
        .merge(json!({ "age": 38, "site": "corporate" }))
        .merge_eval(|value| json!({ "slug": format_value(value, "{name}").to_lowercase() }))
        .delete("date")
        .add("site", "corporate")
        .unwrap();

    assert_eq!(
        value,
        json!({
            "name": "Ada",
            "age": 37,
            "site": "corporate",
            "slug": "ada"
        })
    );
}

#[test]
fn try_map_transforms_direct_values_with_fallible_functions() {
    fn sub_pipeline(value: Value) -> Result<Value> {
        value
            .select(["name", "age"])
            .merge_eval(|value| json!({ "slug": format_value(value, "{name}") }))
    }

    let value = json!({
        "name": "Ada",
        "age": 37,
        "ignored": true
    })
    .try_map(sub_pipeline)
    .unwrap();

    assert_eq!(
        value,
        json!({
            "name": "Ada",
            "age": 37,
            "slug": "Ada"
        })
    );
}

#[test]
fn try_map_transforms_result_pipelines_with_fallible_functions() {
    fn sub_pipeline(value: Value) -> Result<Value> {
        value.add("site", "corporate")
    }

    let value = serde_json::from_str::<Value>(r#"{"name":"Ada"}"#)
        .try_map(sub_pipeline)
        .unwrap();

    assert_eq!(
        value,
        json!({
            "name": "Ada",
            "site": "corporate"
        })
    );
}

#[test]
fn delete_removes_top_level_fields() {
    let value = json!({
        "name": "Ada",
        "secret": true
    })
    .delete("secret")
    .unwrap();

    assert_eq!(value, json!({ "name": "Ada" }));
}

#[test]
fn delete_removes_json_pointer_object_fields() {
    let value = json!({
        "metadata": {
            "department": "research",
            "secret": true
        }
    })
    .delete("/metadata/secret")
    .unwrap();

    assert_eq!(
        value,
        json!({
            "metadata": {
                "department": "research"
            }
        })
    );
}

#[test]
fn delete_removes_json_pointer_array_items() {
    let value = json!({
        "items": ["first", "second", "third"]
    })
    .delete("/items/1")
    .unwrap();

    assert_eq!(value, json!({ "items": ["first", "third"] }));
}

#[test]
fn delete_missing_fields_is_a_no_op() {
    let value = json!({ "name": "Ada" });

    assert_eq!(value.clone().delete("missing").unwrap(), value);
    assert_eq!(value.clone().delete("/missing/path").unwrap(), value);
}

#[test]
fn rename_moves_top_level_fields_and_overwrites_target() {
    let value = json!({
        "name": "Ada",
        "display_name": "old"
    })
    .rename("name", "display_name")
    .unwrap();

    assert_eq!(value, json!({ "display_name": "Ada" }));
}

#[test]
fn rename_moves_json_pointer_fields_to_nested_targets() {
    let value = json!({
        "person": {
            "name": "Ada",
            "age": 37
        }
    })
    .rename("/person/name", "/metadata/display_name")
    .unwrap();

    assert_eq!(
        value,
        json!({
            "person": {
                "age": 37
            },
            "metadata": {
                "display_name": "Ada"
            }
        })
    );
}

#[test]
fn rename_missing_sources_is_a_no_op() {
    let value = json!({ "name": "Ada" });

    assert_eq!(value.clone().rename("missing", "name").unwrap(), value);
    assert_eq!(
        value.clone().rename("/missing/path", "name").unwrap(),
        value
    );
}

#[test]
fn copy_copies_top_level_fields_and_preserves_source() {
    let value = json!({
        "name": "Ada"
    })
    .copy("name", "display_name")
    .unwrap();

    assert_eq!(
        value,
        json!({
            "name": "Ada",
            "display_name": "Ada"
        })
    );
}

#[test]
fn copy_copies_json_pointer_fields_to_nested_targets() {
    let value = json!({
        "person": {
            "name": "Ada"
        }
    })
    .copy("/person/name", "/metadata/display_name")
    .unwrap();

    assert_eq!(
        value,
        json!({
            "person": {
                "name": "Ada"
            },
            "metadata": {
                "display_name": "Ada"
            }
        })
    );
}

#[test]
fn copy_missing_sources_is_a_no_op() {
    let value = json!({ "name": "Ada" });

    assert_eq!(
        value.clone().copy("missing", "display_name").unwrap(),
        value
    );
    assert_eq!(
        value.clone().copy("/missing/path", "display_name").unwrap(),
        value
    );
}

#[test]
fn rename_and_copy_work_in_result_pipelines() {
    let value = serde_json::from_str::<Value>(r#"{"name":"Ada"}"#)
        .copy("name", "display_name")
        .rename("name", "/person/name")
        .unwrap();

    assert_eq!(
        value,
        json!({
            "display_name": "Ada",
            "person": {
                "name": "Ada"
            }
        })
    );
}

#[test]
fn validate_as_returns_original_value_when_deserialization_succeeds() {
    #[allow(dead_code)]
    #[derive(Deserialize)]
    struct Input {
        name: String,
        age: i64,
    }

    let value = json!({
        "name": "Ada",
        "age": 37,
        "extra": true
    });

    let validated = value.clone().validate_as::<Input>().unwrap();

    assert_eq!(validated, value);
}

#[test]
fn validate_as_returns_deserialization_errors() {
    #[allow(dead_code)]
    #[derive(Deserialize)]
    struct Input {
        name: String,
        age: i64,
    }

    let error = json!({
        "name": "Ada",
        "age": "old"
    })
    .validate_as::<Input>()
    .unwrap_err();

    assert!(matches!(error, Error::Json(_)));
    assert!(error.to_string().contains("invalid type"));
}

#[test]
fn require_shape_returns_original_value_when_shape_matches() {
    let value = json!({
        "name": "Ada",
        "age": 37,
        "active": true,
        "metadata": {
            "department": "research",
            "ignored": true
        },
        "tags": ["math", "code"],
        "extra": false
    });

    let validated = value
        .clone()
        .require_shape(json!({
            "name": "",
            "age": 0,
            "active": false,
            "metadata": {
                "department": ""
            },
            "tags": []
        }))
        .unwrap();

    assert_eq!(validated, value);
}

#[test]
fn require_shape_fails_for_missing_fields() {
    let error = json!({
        "name": "Ada"
    })
    .require_shape(json!({
        "name": "",
        "age": 0
    }))
    .unwrap_err();

    assert!(matches!(error, Error::Shape(_)));
    assert!(error.to_string().contains("$.age: missing required field"));
}

#[test]
fn require_shape_fails_for_wrong_field_types() {
    let error = json!({
        "name": "Ada",
        "age": "old"
    })
    .require_shape(json!({
        "name": "",
        "age": 0
    }))
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("$.age: expected number, found string")
    );
}

#[test]
fn require_shape_recurses_through_objects() {
    let error = json!({
        "metadata": {
            "department": 42
        }
    })
    .require_shape(json!({
        "metadata": {
            "department": ""
        }
    }))
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("$.metadata.department: expected string, found number")
    );
}

#[test]
fn require_shape_requires_arrays_but_not_array_element_shapes() {
    let value = json!({
        "tags": [1, true, "mixed"]
    });

    assert_eq!(
        value
            .clone()
            .require_shape(json!({ "tags": [""] }))
            .unwrap(),
        value
    );

    let error = json!({ "tags": "not an array" })
        .require_shape(json!({ "tags": [] }))
        .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("$.tags: expected array, found string")
    );
}

#[test]
fn select_shape_requires_shape_and_removes_extra_fields() {
    let value = json!({
        "name": "Ada",
        "age": 37,
        "metadata": {
            "department": "research",
            "secret": true
        },
        "ignored": true
    })
    .select_shape(json!({
        "name": "",
        "metadata": {
            "department": ""
        }
    }))
    .unwrap();

    assert_eq!(
        value,
        json!({
            "name": "Ada",
            "metadata": {
                "department": "research"
            }
        })
    );
}

#[test]
fn select_shape_fails_when_required_shape_does_not_match() {
    let error = json!({
        "name": "Ada",
        "age": "old"
    })
    .select_shape(json!({
        "name": "",
        "age": 0
    }))
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("$.age: expected number, found string")
    );
}

#[test]
fn format_value_expands_string_fields() {
    let value = json!({
        "first_name": "Ada",
        "last_name": "Lovelace"
    });

    assert_eq!(
        format_value(&value, "{first_name} {last_name}"),
        "Ada Lovelace"
    );
}

#[test]
fn format_value_expands_unknown_and_null_fields_as_empty_strings() {
    let value = json!({
        "name": "Ada",
        "nickname": null
    });

    assert_eq!(format_value(&value, "{name}:{unknown}:{nickname}"), "Ada::");
}

#[test]
fn format_value_expands_non_string_fields_as_json() {
    let value = json!({
        "age": 37,
        "active": true,
        "tags": ["math", "code"]
    });

    assert_eq!(
        format_value(&value, "{age}:{active}:{tags}"),
        r#"37:true:["math","code"]"#
    );
}

#[test]
fn format_value_expands_json_pointer_fields() {
    let value = json!({
        "name": {
            "first": "Ada",
            "last": "Lovelace"
        },
        "sites": ["personal", "corporate"]
    });

    assert_eq!(
        format_value(&value, "{/name/first} {/name/last}:{/sites/1}"),
        "Ada Lovelace:corporate"
    );
}

#[test]
fn format_value_expands_unknown_and_null_json_pointers_as_empty_strings() {
    let value = json!({
        "name": {
            "first": null
        }
    });

    assert_eq!(format_value(&value, "{/missing}{/name/first}"), "");
}

#[test]
fn format_value_supports_escaped_json_pointer_tokens() {
    let value = json!({
        "a/b": {
            "c~d": "found"
        }
    });

    assert_eq!(format_value(&value, "{/a~1b/c~0d}"), "found");
}

#[test]
fn format_value_leaves_unmatched_opening_braces_literal() {
    let value = json!({ "name": "Ada" });

    assert_eq!(format_value(&value, "Hello {name"), "Hello {name");
}

#[test]
fn parse_string_extracts_top_level_fields() {
    let value = parse_string(
        "Ada Lovelace (corporate)",
        "{first_name} {last_name} ({site})",
    )
    .unwrap();

    assert_eq!(
        value,
        json!({
            "first_name": "Ada",
            "last_name": "Lovelace",
            "site": "corporate"
        })
    );
}

#[test]
fn parse_string_extracts_json_pointer_fields_as_nested_objects() {
    let value = parse_string(
        "Ada Lovelace (corporate)",
        "{/person/first_name} {/person/last_name} ({/site/name})",
    )
    .unwrap();

    assert_eq!(
        value,
        json!({
            "person": {
                "first_name": "Ada",
                "last_name": "Lovelace"
            },
            "site": {
                "name": "corporate"
            }
        })
    );
}

#[test]
fn parse_string_supports_escaped_json_pointer_tokens() {
    let value = parse_string("found", "{/a~1b/c~0d}").unwrap();

    assert_eq!(
        value,
        json!({
            "a/b": {
                "c~d": "found"
            }
        })
    );
}

#[test]
fn parse_string_captures_to_end_when_field_is_last() {
    let value = parse_string("name: Ada Lovelace", "name: {name}").unwrap();

    assert_eq!(value, json!({ "name": "Ada Lovelace" }));
}

#[test]
fn parse_string_later_repeated_fields_replace_earlier_ones() {
    let value = parse_string("first=A second=B", "first={name} second={name}").unwrap();

    assert_eq!(value, json!({ "name": "B" }));
}

#[test]
fn parse_string_returns_error_when_literals_do_not_match() {
    let error = parse_string("Ada Lovelace", "{first_name}, {last_name}").unwrap_err();

    assert!(matches!(error, Error::ParseString(_)));
    assert!(error.to_string().contains("expected literal"));
}

#[test]
fn parse_string_returns_error_for_unmatched_opening_braces() {
    let error = parse_string("Ada", "{name").unwrap_err();

    assert!(error.to_string().contains("unmatched opening brace"));
}

#[test]
fn parse_string_returns_error_for_empty_placeholders() {
    let error = parse_string("Ada", "{}").unwrap_err();

    assert!(error.to_string().contains("empty field placeholder"));
}

#[test]
fn typed_getters_return_fields() {
    let value = json!({
        "name": "Ada",
        "age": 37,
        "score": 99.5,
        "active": true
    });

    assert_eq!(value.get_str("name", "unknown"), "Ada");
    assert_eq!(value.get_i64("age", 0), 37);
    assert_eq!(value.get_f64("score", 0.0), 99.5);
    assert_eq!(value.get_bool("active", false), true);
}

#[test]
fn typed_getters_return_defaults_for_missing_null_and_wrong_type_fields() {
    let value = json!({
        "name": null,
        "age": "old",
        "score": false,
        "active": 1
    });

    assert_eq!(value.get_str("name", "unknown"), "unknown");
    assert_eq!(value.get_i64("age", 0), 0);
    assert_eq!(value.get_f64("score", 1.5), 1.5);
    assert_eq!(value.get_bool("active", false), false);
    assert_eq!(json!(42).get_str("name", "unknown"), "unknown");
}

#[test]
fn typed_getters_support_json_pointer_keys() {
    let value = json!({
        "metadata": {
            "name": "Ada",
            "active": true
        }
    });

    assert_eq!(value.get_str("/metadata/name", "unknown"), "Ada");
    assert_eq!(value.get_bool("/metadata/active", false), true);
    assert_eq!(value.get_str("/metadata/missing", "unknown"), "unknown");
}

#[test]
fn collection_getters_return_fields_or_defaults() {
    let value = json!({
        "items": [1, 2],
        "metadata": {
            "source": "archive"
        }
    });
    let default_array = vec![json!("default")];
    let mut default_object = Map::new();
    default_object.insert("default".to_owned(), json!(true));

    assert_eq!(
        value.get_array("items", &default_array),
        &[json!(1), json!(2)]
    );
    assert_eq!(
        value.get_object("metadata", &default_object).get("source"),
        Some(&json!("archive"))
    );
    assert_eq!(value.get_array("missing", &default_array), default_array);
    assert_eq!(
        value.get_object("missing", &default_object),
        &default_object
    );
}

#[test]
fn optional_collection_getters_return_fields_or_none() {
    let value = json!({
        "items": [1, 2],
        "metadata": {
            "source": "archive"
        },
        "not_items": false,
        "not_metadata": "archive"
    });

    assert_eq!(
        value.get_array_opt("items"),
        Some(&[json!(1), json!(2)][..])
    );
    assert_eq!(
        value
            .get_object_opt("metadata")
            .and_then(|object| object.get("source")),
        Some(&json!("archive"))
    );
    assert_eq!(value.get_array_opt("missing"), None);
    assert_eq!(value.get_array_opt("not_items"), None);
    assert_eq!(value.get_object_opt("missing"), None);
    assert_eq!(value.get_object_opt("not_metadata"), None);
}
