use rodio::{OutputStream, Sink};
use std::net::UdpSocket;
use std::sync::{Arc, Mutex};
use std::thread;

use crate::player::{add_tracks_to_sink, display_progress, load_audio_files};
use crate::track::Track;
use crate::utils;

/// Executes the member's role in the synchronization process.
/// 
/// This function coordinates the synchronization process for a member device. It listens for 
/// broadcast messages from the leader, establishes a connection, and synchronizes audio playback.
/// 
/// # Steps
/// 1. Binds to a specified UDP port and listens for leader broadcasts.
/// 2. Responds to leader pings and establishes communication.
/// 3. Starts a user input thread to send playback commands to the leader.
/// 4. Loads audio tracks and displays playback progress.
/// 5. Listens for synchronization messages from the leader to control playback.
pub fn run_member() -> std::io::Result<()> {
    let socket = UdpSocket::bind("0.0.0.0:12345")?;
    println!("Welcome to SyncStream!\nListening for broadcasts...");

    let mut last_received_id = 0;
    let leader_addr = Arc::new(Mutex::new(None));

    loop {
        let mut buf = [0u8; 1024];
        let (size, src) = socket.recv_from(&mut buf)?;
        let message = String::from_utf8_lossy(&buf[..size]);

        if message.starts_with("PING") {
            handle_ping_message(&message, &mut last_received_id, &leader_addr, &socket, src)?;
        } else if message == "Done broadcasting" {
            break;
        }
    }

    spawn_user_input_thread(socket.try_clone()?, Arc::clone(&leader_addr));

    let (_stream, stream_handle) = OutputStream::try_default().unwrap();
    let sink = Arc::new(Mutex::new(Sink::try_new(&stream_handle).unwrap()));
    sink.lock().unwrap().pause(); // To prevent playing before synchronization

    let mut tracks = Vec::<Track>::new();
    load_audio_files("media", &mut tracks);

    // Wait for a message from the leader for the list of track names
    let mut buf = [0u8; 1024];
    let (size, _) = socket.recv_from(&mut buf)?;
    let message = String::from_utf8_lossy(&buf[..size]);
    let selected_tracks: Vec<&str> = message.split(':').collect();
    let selected_tracks: Vec<&str> = selected_tracks[1].split(',').collect();

    let tracks: Vec<Track> = tracks
        .into_iter()
        .filter(|track| selected_tracks.contains(&&track.name.as_str()))
        .collect();

    add_tracks_to_sink("media", Arc::clone(&sink), &tracks);

    let current_track_index = Arc::new(Mutex::new(0));
    let should_reset = Arc::new(Mutex::new(false));

    display_progress(
        Arc::clone(&sink),
        tracks.clone(),
        Arc::clone(&current_track_index),
        Arc::clone(&should_reset),
    );

    utils::start_track_position_thread(
        sink.clone(),
        current_track_index.clone(),
        should_reset.clone(),
        tracks.clone(),
    );

    handle_incoming_messages(socket, sink, tracks, current_track_index, should_reset)
}

/// Handles incoming PING messages from the leader.
/// 
/// This function processes PING messages from the leader, determines if the leader's ID 
/// is valid, and responds with an ACK message to establish a connection.
fn handle_ping_message(
    message: &str,
    last_received_id: &mut u64,
    leader_addr: &Arc<Mutex<Option<std::net::SocketAddr>>>,
    socket: &UdpSocket,
    src: std::net::SocketAddr,
) -> std::io::Result<()> {
    let parts: Vec<&str> = message.split(',').collect();
    if let Ok(id) = parts[1].parse::<u64>() {
        if id > *last_received_id {
            *last_received_id = id;
            if leader_addr.lock().unwrap().is_none() {
                socket.send_to(b"ACK", src)?;
                println!("Connected to leader at {}", src);
                *leader_addr.lock().unwrap() = Some(src);
            }
        }
    }
    Ok(())
}

/// Spawns a thread to handle user input and send commands to the leader.
/// 
/// This function continuously reads user input and sends supported commands (`p`, `n`, `r`, `s`) 
/// to the leader via UDP. If the leader address is not known, it informs the user to wait.
fn spawn_user_input_thread(
    socket: UdpSocket,
    leader_addr: Arc<Mutex<Option<std::net::SocketAddr>>>,
) {
    thread::spawn(move || loop {
        let mut input = String::new();
        if std::io::stdin().read_line(&mut input).is_ok() {
            let trimmed = input.trim();
            if let Some(addr) = *leader_addr.lock().unwrap() {
                match trimmed {
                    "p" | "n" | "s" | "r" => {
                        if let Err(e) = socket.send_to(trimmed.as_bytes(), addr) {
                            eprintln!("Failed to send input to leader: {}", e);
                        }
                    }
                    _ => println!("Unknown command. Use 'p', 'n', 'r', or 's'."),
                }
            } else {
                println!("Leader address not known yet. Please wait.");
            }
        }
    });
}

/// Listens for and processes synchronization messages from the leader.
/// 
/// This function continuously listens for messages from the leader to synchronize 
/// playback. It extracts the timestamp and playback mode from each message and 
/// executes the corresponding action.
fn handle_incoming_messages(
    socket: UdpSocket,
    sink: Arc<Mutex<Sink>>,
    tracks: Vec<Track>,
    current_track_index: Arc<Mutex<usize>>,
    should_reset: Arc<Mutex<bool>>,
) -> std::io::Result<()> {
    let mut buf = [0u8; 1024];
    loop {
        match socket.recv_from(&mut buf) {
            Ok((size, _)) => {
                let message = String::from_utf8_lossy(&buf[..size]);

                if let Some(timestamp) = utils::extract_timestamp(&message) {
                    if let Some(mode) = utils::extract_mode(&message) {
                        handle_mode(
                            mode,
                            timestamp,
                            &sink,
                            &tracks,
                            &current_track_index,
                            &should_reset,
                        );
                    } else {
                        println!("Failed to extract a mode.");
                    }
                } else {
                    println!("Failed to extract a timestamp.");
                }
            }
            Err(e) => eprintln!("Error receiving: {}", e),
        }
    }
}

/// Executes a playback command based on the received mode and timestamp.
/// 
/// This function synchronizes playback by executing the specified mode (play, pause, 
/// next track, restart track, stop) at the given timestamp.
fn handle_mode(
    mode: u64,
    timestamp: u64,
    sink: &Arc<Mutex<Sink>>,
    tracks: &[Track],
    current_track_index: &Arc<Mutex<usize>>,
    should_reset: &Arc<Mutex<bool>>,
) {
    match mode {
        0 => utils::synchronized_action("p", timestamp, sink),
        2 => {
            {
                let mut track_index = current_track_index.lock().unwrap();
                *track_index += 1;
                if *track_index >= tracks.len() {
                    println!("\nNo more tracks!");
                    println!("Thanks for using the SyncStream!");
                    std::process::exit(0);
                }
            }
            utils::synchronized_action("n", timestamp, sink);
            *should_reset.lock().unwrap() = true;
        }
        3 => utils::synchronized_action("s", timestamp, sink),
        4 => utils::synchronized_action("r", timestamp, sink),
        _ => {}
    }
}
