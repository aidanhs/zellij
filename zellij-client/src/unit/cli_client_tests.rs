use super::individual_messages_client;
use crate::os_input_output::ClientOsApi;
use std::collections::VecDeque;
use std::io::{BufRead, Cursor, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use zellij_utils::{
    data::Palette,
    errors::ErrorContext,
    input::actions::Action,
    ipc::{ClientToServerMsg, IpcReceiveError, ServerToClientMsg},
    pane_size::Size,
};

/// Replays scripted receive results, then reports the server as gone. A
/// closed connection stays closed, so receiving again after `Disconnected`
/// means the caller is spinning on it.
#[derive(Clone, Debug)]
struct ScriptedServerOsInput {
    script: Arc<Mutex<VecDeque<Result<ServerToClientMsg, IpcReceiveError>>>>,
    disconnected: Arc<Mutex<bool>>,
}

impl ScriptedServerOsInput {
    fn new(script: Vec<Result<ServerToClientMsg, IpcReceiveError>>) -> Self {
        ScriptedServerOsInput {
            script: Arc::new(Mutex::new(script.into())),
            disconnected: Arc::new(Mutex::new(false)),
        }
    }
}

impl ClientOsApi for ScriptedServerOsInput {
    fn get_terminal_size(&self) -> Size {
        Size::default()
    }
    fn set_raw_mode(&mut self) {}
    fn unset_raw_mode(&self) -> Result<(), std::io::Error> {
        Ok(())
    }
    fn get_stdout_writer(&self) -> Box<dyn Write> {
        Box::new(std::io::sink())
    }
    fn get_stdin_reader(&self) -> Box<dyn BufRead> {
        Box::new(Cursor::new(Vec::<u8>::new()))
    }
    fn update_session_name(&mut self, _new_session_name: String) {}
    fn read_from_stdin(&mut self) -> Result<Vec<u8>, &'static str> {
        Ok(vec![])
    }
    fn box_clone(&self) -> Box<dyn ClientOsApi> {
        Box::new(self.clone())
    }
    fn send_to_server(&self, _msg: ClientToServerMsg) {}
    fn recv_from_server(&self) -> Option<(ServerToClientMsg, ErrorContext)> {
        self.try_recv_from_server().ok()
    }
    fn try_recv_from_server(&self) -> Result<(ServerToClientMsg, ErrorContext), IpcReceiveError> {
        let mut disconnected = self.disconnected.lock().unwrap();
        assert!(
            !*disconnected,
            "received again after the server connection was reported closed"
        );
        let next = self
            .script
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Err(IpcReceiveError::Disconnected));
        *disconnected = matches!(next, Err(IpcReceiveError::Disconnected));
        next.map(|msg| (msg, ErrorContext::default()))
    }
    fn handle_signals(
        &self,
        _sigwinch_cb: Box<dyn Fn()>,
        _quit_cb: Box<dyn Fn()>,
        _resize_receiver: Option<std::sync::mpsc::Receiver<()>>,
    ) {
    }
    fn connect_to_server(&self, _path: &Path) {}
    fn load_palette(&self) -> Palette {
        Palette::default()
    }
    fn enable_mouse(&self) -> anyhow::Result<()> {
        Ok(())
    }
    fn disable_mouse(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

#[test]
fn individual_messages_client_exits_with_an_error_when_the_server_disconnects() {
    let mut os_input: Box<dyn ClientOsApi> = Box::new(ScriptedServerOsInput::new(vec![]));

    let exit_status = individual_messages_client(&mut os_input, Action::NoOp, None);

    assert_eq!(exit_status, Some(2));
}

#[test]
fn individual_messages_client_skips_an_undecodable_message() {
    let mut os_input: Box<dyn ClientOsApi> = Box::new(ScriptedServerOsInput::new(vec![
        Err(IpcReceiveError::Undecodable),
        Ok(ServerToClientMsg::UnblockInputThread),
    ]));

    let exit_status = individual_messages_client(&mut os_input, Action::NoOp, None);

    assert_eq!(exit_status, None);
}
