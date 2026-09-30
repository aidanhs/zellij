use crate::os_input_output::get_server_os_input;
use crate::start_server_impl;
use zellij_utils::consts::ipc_connect;
use zellij_utils::ipc::{
    ClientToServerMsg, IpcReceiverWithContext, IpcSenderWithContext, ServerToClientMsg,
};

fn connect(socket_path: &std::path::Path) -> IpcSenderWithContext<ClientToServerMsg> {
    // The listener thread binds the socket asynchronously after the server starts.
    loop {
        match ipc_connect(socket_path) {
            Ok(stream) => return IpcSenderWithContext::new(stream),
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(1)),
        }
    }
}

#[test]
fn server_survives_a_client_that_disconnects_before_first_client_connected() {
    let socket_dir = tempfile::tempdir().unwrap();
    let socket_path = socket_dir.path().join("session");
    let os_input = get_server_os_input().unwrap();
    let server_thread = std::thread::spawn({
        let socket_path = socket_path.clone();
        move || {
            let install_panic_hook = false;
            start_server_impl(Box::new(os_input), socket_path, install_panic_hook);
        }
    });

    // What a session-discovery probe does against a server whose creating
    // client has not sent FirstClientConnected yet: ConnStatus, read the reply,
    // hang up. Reading until EOF means the probe's route thread has exited, so
    // its RemoveClient is already queued on the server.
    let mut probe = connect(&socket_path);
    probe
        .send_client_msg(ClientToServerMsg::ConnStatus)
        .unwrap();
    let mut probe_receiver: IpcReceiverWithContext<ServerToClientMsg> = probe.get_receiver();
    assert!(matches!(
        probe_receiver.recv_server_msg(),
        Some((ServerToClientMsg::Connected, _))
    ));
    while probe_receiver.recv_server_msg().is_some() {}

    // The server handles instructions in order, so KillSession is only reached
    // if the probe's RemoveClient was handled without panicking.
    let mut killer = connect(&socket_path);
    killer
        .send_client_msg(ClientToServerMsg::KillSession)
        .unwrap();
    assert!(
        server_thread.join().is_ok(),
        "the server thread panicked while removing a client before the session existed"
    );
}
