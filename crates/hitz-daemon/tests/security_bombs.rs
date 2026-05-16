//! Security tests for hitz-daemon
use bytes::Bytes;
use http_body_util::{BodyExt, Full, Limited};

#[tokio::test]
async fn test_large_body_exploit() {
    let size = 11 * 1024 * 1024;
    let payload = vec![0u8; size];
    let body = Full::new(Bytes::from(payload));

    let limited = Limited::new(body, 10 * 1024 * 1024);

    let result = limited.collect().await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.to_string(), "length limit exceeded");
}
