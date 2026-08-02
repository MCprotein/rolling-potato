use std::time::Duration;

use super::protocol::ChatSseDecoder;
use crate::runtime_core::inference::stream::DecodedFinish;

#[test]
fn decodes_length_finish_as_typed_incomplete_signal() {
    let mut decoder = ChatSseDecoder::default();
    decoder
        .push(
            b"data: {\"choices\":[{\"delta\":{\"content\":\"partial\"},\"finish_reason\":\"length\"}]}\n\ndata: [DONE]\n\n",
            Duration::from_millis(1),
            &mut |_| Ok(()),
        )
        .unwrap();

    let completion = decoder.completion();

    assert_eq!(completion.decoded_finish, DecodedFinish::Length);
    assert_eq!(completion.raw_finish_reason.as_deref(), Some("length"));
    assert_eq!(completion.finish_reason, "length");
}

#[test]
fn preserves_unknown_finish_reason_as_diagnostic_only() {
    let mut decoder = ChatSseDecoder::default();
    decoder
        .push(
            b"data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"content_filter\"}]}\n\ndata: [DONE]\n\n",
            Duration::from_millis(1),
            &mut |_| Ok(()),
        )
        .unwrap();

    let completion = decoder.completion();

    assert_eq!(completion.decoded_finish, DecodedFinish::UnknownOrMissing);
    assert_eq!(
        completion.raw_finish_reason.as_deref(),
        Some("content_filter")
    );
    assert_eq!(completion.finish_reason, "content_filter");
}

#[test]
fn maps_missing_finish_reason_to_typed_unknown() {
    let mut decoder = ChatSseDecoder::default();
    decoder
        .push(
            b"data: {\"choices\":[{\"delta\":{\"content\":\"answer\"}}]}\n\ndata: [DONE]\n\n",
            Duration::from_millis(1),
            &mut |_| Ok(()),
        )
        .unwrap();

    let completion = decoder.completion();

    assert_eq!(completion.decoded_finish, DecodedFinish::UnknownOrMissing);
    assert_eq!(completion.raw_finish_reason, None);
    assert_eq!(completion.finish_reason, "unknown");
}
