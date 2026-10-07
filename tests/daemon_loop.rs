//! The daemon loop end to end over the in-process transport: snapshot on
//! connect, a request in, process output back, clean shutdown when the TUI
//! hangs up.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::time::Duration;

use paddock::config::Config;
use paddock::ipc::protocol::{Event, Request};
use paddock::ipc::transport;
use paddock::model::ProcessId;
use tokio::time::timeout;

#[tokio::test(flavor = "multi_thread")]
async fn start_streams_output_and_hanging_up_stops_everything() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("proj");
    fs::create_dir(&project).unwrap();
    fs::write(project.join("Procfile"), "app: echo hello-loop; sleep 30\n").unwrap();
    let config = Config {
        projects: vec![project.clone()],
        ..Config::default()
    };

    let (mut client, server) = transport::in_process();
    let daemon = tokio::spawn(paddock::daemon::run(
        server,
        config,
        dir.path().join("config.toml"),
    ));

    let first = timeout(Duration::from_secs(5), client.events.recv())
        .await
        .unwrap();
    assert!(matches!(first, Some(Event::Snapshot(_))), "{first:?}");

    client
        .requests
        .send(Request::Start(ProcessId::new("proj", "app")))
        .await
        .unwrap();
    let saw_output = timeout(Duration::from_secs(10), async {
        while let Some(event) = client.events.recv().await {
            if let Event::Output { lines, .. } = event
                && lines.iter().any(|l| l.contains("hello-loop"))
            {
                return true;
            }
        }
        false
    })
    .await
    .unwrap_or(false);
    assert!(saw_output, "no output from the started process");

    drop(client);
    timeout(Duration::from_secs(8), daemon)
        .await
        .expect("daemon did not shut down")
        .unwrap();
}
