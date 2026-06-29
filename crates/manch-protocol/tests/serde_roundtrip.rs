//! Property tests: the protocol's own serializable types must survive a JSON
//! round-trip unchanged. A stable wire contract is the reason this crate exists.

use agent_client_protocol::schema::v1::{ContentBlock, TextContent, ToolKind};
use manch_protocol::{Context, ToolSchema};
use proptest::prelude::*;

/// Arbitrary JSON values, deliberately excluding floats so equality is exact
/// (no NaN, no precision loss). Integers, strings, bools, null, nested arrays
/// and objects only.
fn arb_json() -> impl Strategy<Value = serde_json::Value> {
    let leaf = prop_oneof![
        Just(serde_json::Value::Null),
        any::<bool>().prop_map(serde_json::Value::Bool),
        any::<i64>().prop_map(|n| serde_json::Value::Number(n.into())),
        any::<String>().prop_map(serde_json::Value::String),
    ];
    leaf.prop_recursive(3, 16, 5, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..5).prop_map(serde_json::Value::Array),
            prop::collection::vec((any::<String>(), inner), 0..5)
                .prop_map(|kvs| serde_json::Value::Object(kvs.into_iter().collect())),
        ]
    })
}

fn arb_tool_kind() -> impl Strategy<Value = ToolKind> {
    prop_oneof![
        Just(ToolKind::Read),
        Just(ToolKind::Edit),
        Just(ToolKind::Delete),
        Just(ToolKind::Move),
        Just(ToolKind::Search),
        Just(ToolKind::Execute),
        Just(ToolKind::Think),
        Just(ToolKind::Fetch),
        Just(ToolKind::SwitchMode),
        Just(ToolKind::Other),
    ]
}

fn arb_tool_schema() -> impl Strategy<Value = ToolSchema> {
    (
        any::<String>(),
        any::<String>(),
        arb_tool_kind(),
        arb_json(),
    )
        .prop_map(|(name, description, kind, input_schema)| ToolSchema {
            name,
            description,
            kind,
            input_schema,
        })
}

fn arb_context() -> impl Strategy<Value = Context> {
    (
        any::<String>(),
        prop::collection::vec(any::<String>(), 0..5),
    )
        .prop_map(|(session_id, texts)| Context {
            session_id,
            blocks: texts
                .into_iter()
                .map(|t| ContentBlock::Text(TextContent::new(t)))
                .collect(),
        })
}

proptest! {
    #[test]
    fn tool_schema_json_roundtrip(schema in arb_tool_schema()) {
        let json = serde_json::to_string(&schema).unwrap();
        let back: ToolSchema = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(schema, back);
    }

    #[test]
    fn context_json_roundtrip(ctx in arb_context()) {
        let json = serde_json::to_string(&ctx).unwrap();
        let back: Context = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(ctx, back);
    }
}
