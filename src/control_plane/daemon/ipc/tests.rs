use super::{
    IPC_PROTOCOL_VERSION, IpcPayload, IpcRequest, IpcResponse, IpcResult, decode_request_frame,
    decode_response_frame, encode_frame,
};
use std::path::PathBuf;

#[test]
fn request_frames_round_trip_with_version_id_and_typed_payload() {
    let request = IpcRequest::new(
        "request-42",
        IpcPayload::ReconcileProject {
            project_path: PathBuf::from("/work/bill"),
        },
    );

    let frame = encode_frame(&request).expect("encode request");
    let decoded = decode_request_frame(&frame).expect("decode request");

    assert!(frame.ends_with(b"\n"));
    assert_eq!(decoded, request);
    assert_eq!(decoded.protocol_version(), IPC_PROTOCOL_VERSION);
    assert_eq!(decoded.request_id(), "request-42");
}

#[test]
fn cancellation_targets_an_existing_request_id() {
    let request = IpcRequest::new(
        "cancel-1",
        IpcPayload::Cancel {
            target_request_id: "request-42".to_owned(),
        },
    );

    let frame = encode_frame(&request).expect("encode cancellation");
    let decoded = decode_request_frame(&frame).expect("decode cancellation");

    assert_eq!(decoded, request);
}

#[test]
fn typed_response_frames_preserve_request_correlation() {
    let response = IpcResponse::success("request-42", IpcResult::Pong);

    let frame = encode_frame(&response).expect("encode response");
    let decoded = decode_response_frame(&frame).expect("decode response");

    assert_eq!(decoded, response);
    assert_eq!(decoded.request_id(), "request-42");
}

#[test]
fn event_subscriptions_can_resume_after_a_sequence() {
    let request = IpcRequest::new(
        "events-1",
        IpcPayload::SubscribeEvents {
            after_sequence: Some(41),
        },
    );

    let frame = encode_frame(&request).expect("encode subscription");
    let decoded = decode_request_frame(&frame).expect("decode subscription");

    assert_eq!(decoded, request);
}

#[test]
fn unsupported_protocol_versions_fail_before_dispatch() {
    let frame = br#"{"protocol_version":99,"request_id":"request-1","payload":{"type":"ping"}}
"#;

    let error = decode_request_frame(frame).expect_err("unsupported protocol");

    assert_eq!(
        error.to_string(),
        "IPC protocol version 99 is unsupported; expected 1"
    );
}

#[test]
fn unknown_request_fields_are_rejected() {
    let frame = br#"{"protocol_version":1,"request_id":"request-1","surprise":true,"payload":{"type":"ping"}}
"#;

    let error = decode_request_frame(frame).expect_err("unknown field");

    assert!(error.to_string().contains("unknown field `surprise`"));
}

#[cfg(unix)]
#[test]
fn unix_listener_is_user_only_and_exclusive() {
    use super::UnixIpcListener;
    use std::os::unix::fs::PermissionsExt;
    use std::time::{SystemTime, UNIX_EPOCH};

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after epoch")
        .as_nanos();
    let socket_path = std::env::temp_dir().join(format!(
        "stackctl-v8-ipc-{}-{unique}.sock",
        std::process::id()
    ));
    let listener = UnixIpcListener::bind(&socket_path).expect("bind IPC listener");
    let mode = std::fs::metadata(&socket_path)
        .expect("socket metadata")
        .permissions()
        .mode()
        & 0o777;

    assert_eq!(mode, 0o600);
    assert!(UnixIpcListener::bind(&socket_path).is_err());

    drop(listener);
    assert!(!socket_path.exists());
}
