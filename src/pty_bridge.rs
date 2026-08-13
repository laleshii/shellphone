use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::io::{Read, Write};
use tokio::sync::{broadcast, mpsc, oneshot};

pub enum PtyCommand {
    Input(Vec<u8>),
    Resize { cols: u16, rows: u16 },
}

pub enum PtyEvent {
    Output(Vec<u8>),
    Exit(Option<u32>),
}

pub type CommandTx = mpsc::Sender<PtyCommand>;
pub type EventRx = broadcast::Receiver<PtyEvent>;
pub type ExitRx = oneshot::Receiver<Option<u32>>;

impl Clone for PtyEvent {
    fn clone(&self) -> Self {
        match self {
            PtyEvent::Output(data) => PtyEvent::Output(data.clone()),
            PtyEvent::Exit(code) => PtyEvent::Exit(*code),
        }
    }
}

pub fn spawn(command: &str) -> anyhow::Result<(CommandTx, EventRx, ExitRx)> {
    let pty_system = native_pty_system();
    let pair = pty_system.openpty(PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    })?;

    let mut cmd = CommandBuilder::new("sh");
    cmd.arg("-c");
    cmd.arg(command);

    let mut child = pair.slave.spawn_command(cmd)?;
    drop(pair.slave);

    let (event_tx, event_rx) = broadcast::channel::<PtyEvent>(256);
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<PtyCommand>(256);

    let mut reader = pair.master.try_clone_reader()?;
    let event_tx2 = event_tx.clone();
    tokio::task::spawn_blocking(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if event_tx2.send(PtyEvent::Output(buf[..n].to_vec())).is_err() {
                        break;
                    }
                }
            }
        }
    });

    let (exit_tx, exit_rx) = oneshot::channel::<Option<u32>>();
    let event_tx3 = event_tx.clone();
    tokio::task::spawn_blocking(move || {
        let status = child.wait();
        let code = status.ok().map(|s| {
            if s.success() {
                0
            } else {
                s.exit_code()
            }
        });
        let _ = event_tx3.send(PtyEvent::Exit(code));
        let _ = exit_tx.send(code);
    });

    let master = pair.master;
    let mut writer = master.take_writer()?;
    tokio::spawn(async move {
        while let Some(cmd) = cmd_rx.recv().await {
            match cmd {
                PtyCommand::Input(data) => {
                    if writer.write_all(&data).is_err() {
                        break;
                    }
                }
                PtyCommand::Resize { cols, rows } => {
                    let _ = master.resize(PtySize {
                        rows,
                        cols,
                        pixel_width: 0,
                        pixel_height: 0,
                    });
                }
            }
        }
    });

    Ok((cmd_tx, event_rx, exit_rx))
}
