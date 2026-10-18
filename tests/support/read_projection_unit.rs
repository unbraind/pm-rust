//! Projection decoding uses real canonical documents and the authoritative decoder.
use super::*;
use serde_json::json;

#[test]
fn projections_keep_required_fields_and_context_counts() -> Result<(), Box<dyn std::error::Error>> {
    let value = json!({"id":"sample-one","title":"Example","description":"long text","type":"Task","status":"open","priority":1,"tags":["alpha"],"created_at":"2026-08-22T10:00:00.000Z","updated_at":"2026-08-22T10:00:00.000Z","parent":"sample-parent","dependencies":[{"id":"sample-other","kind":"blocked_by"}],"deadline":"2026-10-01","comments":[{"text":"many words","author":"sample"}],"notes":[],"learnings":[],"test_runs":[],"files":[{"path":"src/lib.rs"}],"body":"long body"});
    let content = toon_format::encode_default(&value)?;
    let path = Path::new("tasks/sample-one.toon");
    let full = decode(path, &content, Projection::Full)?;
    let list = decode(path, &content, Projection::List)?;
    let brief = decode(path, &content, Projection::Context)?;
    assert_eq!(full.body, "long body");
    assert!(list.body.is_empty());
    assert!(list.metadata.description.is_empty());
    assert_eq!(list.metadata.extra["dependencies"], value["dependencies"]);
    assert!(!list.metadata.extra.contains_key("comments"));
    assert_eq!(
        brief.metadata.extra["comments"].as_array().map(Vec::len),
        Some(1)
    );
    assert_eq!(
        brief.metadata.extra["notes"].as_array().map(Vec::len),
        Some(0)
    );
    assert!(!brief.metadata.extra.contains_key("files"));
    assert!(brief.body.is_empty());
    let empty = content.replace("notes[0]:", "notes: []");
    assert_eq!(
        decode(path, &empty, Projection::Context)?.metadata.extra["notes"],
        json!([])
    );
    Ok(())
}

#[test]
fn uncertain_headers_and_quoted_keys_fall_back_without_guessing()
-> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(collection_count("comments[3]{text}:"), Some(3));
    assert_eq!(collection_count("comments: []"), Some(0));
    for header in ["comments:", "comments[bad]:", "comments[2:"] {
        assert_eq!(collection_count(header), None);
    }
    let content = "id: sample-one\ntitle: Example\ntype: Task\nstatus: open\npriority: 1\ncreated_at: date\nupdated_at: date\ncomments: scalar\nbody: text\n";
    let path = Path::new("sample-one.toon");
    let brief = decode(path, content, Projection::Context)?;
    assert_eq!(brief.metadata.extra["comments"], "scalar");
    let quoted = content.replace("body:", "\"body\":");
    assert_eq!(decode(path, &quoted, Projection::List)?.body, "text");
    for marker in ["<<<<<<< branch", "=======", ">>>>>>> branch"] {
        assert!(decode(path, &format!("{content}{marker}\n"), Projection::List).is_err());
    }
    assert!(decode(path, "title: missing metadata", Projection::List).is_err());
    Ok(())
}

/// Only structurally matching tabular lengths can become count-only arrays.
#[test]
fn count_projection_checks_lengths_and_retains_non_tabular_arrays()
-> Result<(), Box<dyn std::error::Error>> {
    let base = "id: sample-one\ntitle: Example\ntype: Task\nstatus: open\npriority: 1\ncreated_at: date\nupdated_at: date\n";
    let path = Path::new("sample-one.toon");
    let content = format!(
        "{base}comments[2]{{text,author}}:\n  first,sample\n  second,sample\n\nbody: skipped\n"
    );
    assert_eq!(
        decode(path, &content, Projection::Context)?.metadata.extra["comments"],
        json!([null, null])
    );
    assert!(
        decode(
            path,
            &content.replace("comments[2]", "comments[3]"),
            Projection::Context
        )
        .is_err()
    );
    assert_eq!(collection_count("comments[0]:"), None);
    assert_eq!(collection_count("comments[2]{text}"), None);
    let content = format!("{base}comments[2]: first,second\n");
    assert_eq!(
        decode(path, &content, Projection::Context)?.metadata.extra["comments"],
        json!(["first", "second"])
    );
    Ok(())
}
