use crate::{operations, service};
use std::{
    io::Write,
    time::{Duration, Instant},
};

fn arguments(values: &[String]) -> Vec<u8> {
    let mut output = (values.len() as u32).to_le_bytes().to_vec();
    for value in values {
        output.extend_from_slice(&(value.len() as u32).to_le_bytes());
        output.extend_from_slice(value.as_bytes());
    }
    output
}
fn start(mode: &str, timeout: u64, limit: usize) -> i32 {
    let command = std::env::current_exe()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let request = serde_json::json!({"action":"run", "command":command, "options":{
        "env":{"DREAM_PROCESS_HOST_FIXTURE":mode}, "timeout_ms":timeout, "max_output_bytes":limit,
        "input":vec![120u8;200_000],
    }});
    service::start(
        request.to_string(),
        arguments(&[
            "--exact".into(),
            "service_tests::fixture".into(),
            "--nocapture".into(),
        ]),
    )
}
fn complete(id: i32) -> Vec<u8> {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let result = operations::OPERATIONS.result(id);
        if result[0] != 0 {
            operations::OPERATIONS.release(id);
            return result;
        }
        assert!(
            Instant::now() < deadline,
            "process operation did not complete"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}
#[test]
fn fixture() {
    let Ok(mode) = std::env::var("DREAM_PROCESS_HOST_FIXTURE") else {
        return;
    };
    if mode == "sleep" {
        std::thread::sleep(Duration::from_secs(10));
        return;
    }
    std::io::stdout().write_all(&vec![b'a'; 200_000]).unwrap();
    std::io::stderr().write_all(&vec![b'b'; 200_000]).unwrap();
    std::io::copy(&mut std::io::stdin(), &mut std::io::sink()).unwrap();
}
#[test]
fn saturated_pipes_and_input_are_drained_concurrently() {
    let result = complete(start("flood", 5000, 1_000_000));
    assert_eq!(result[0], 1, "{}", String::from_utf8_lossy(&result));
    assert!(result.len() > 400_000);
}
#[test]
fn deadlines_cancellation_and_output_limits_complete() {
    let deadline = complete(start("sleep", 50, 1_000_000));
    assert_eq!(deadline[0], 2);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&deadline[1..]).unwrap()["code"],
        "ETIMEOUT"
    );
    let id = start("sleep", 0, 1_000_000);
    std::thread::sleep(Duration::from_millis(30));
    operations::OPERATIONS.cancel(id);
    let cancelled = complete(id);
    assert_eq!(cancelled[0], 2);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&cancelled[1..]).unwrap()["code"],
        "ECANCELLED"
    );
    let limited = complete(start("flood", 5000, 1000));
    assert_eq!(limited[0], 2);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&limited[1..]).unwrap()["code"],
        "ELIMIT"
    );
}
