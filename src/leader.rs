use asky::Text;
use rodio::{OutputStream, Sink};
use std::collections::HashSet;
use std::io;
use std::net::UdpSocket;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::player::{add_tracks_to_sink, display_progress, load_audio_files};
use crate::track::Track;
use crate::utils;

use asky::MultiSelect;

pub fn run_leader() -> std::io::Result<()> {
    let socket = Arc::new(UdpSocket::bind("0.0.0.0:0")?);
    socket.set_broadcast(true)?;
    let broadcast_addr = "255.255.255.255:12345";

    let members = Arc::new(Mutex::new(HashSet::new()));
    let ping_thread_should_terminate = Arc::new(Mutex::new(false));

    println!("Starting to ping members.");

    let ping_thread = start_ping_thread(
        Arc::clone(&socket),
        broadcast_addr.to_string(),
        Arc::clone(&members),
        Arc::clone(&ping_thread_should_terminate),
    );

    Text::new("Pinging for members. Press ENTER when ready to proceed.").prompt()?;
    stop_ping_thread(
        ping_thread,
        &ping_thread_should_terminate,
        &socket,
        &members,
    )?;

    println!("Final member count: {}", members.lock().unwrap().len());
    Text::new("Press ENTER to start the playback!").prompt()?;
    println!("Commands:\n\t'p' to play/pause\n\t'n' to next\n\t'r' to restart\n\t's' to stop");

    let (_stream, stream_handle) = OutputStream::try_default().unwrap();
    let sink = Arc::new(Mutex::new(Sink::try_new(&stream_handle).unwrap()));
    sink.lock().unwrap().pause(); // To prevent playing before synchronization

    let mut tracks = Vec::<Track>::new();
    load_audio_files("media", &mut tracks);

    let track_names = &tracks
        .iter()
        .map(|track| track.name.clone())
        .collect::<Vec<String>>();

    let selected_tracks = MultiSelect::new(
        "Please select the tracks (with SPACE) you want to include and then confirm with ENTER!",
        track_names,
    )
    .prompt()?;

    // Filter out the selected tracks from tracks variable
    let tracks: Vec<Track> = tracks
        .into_iter()
        .filter(|track| selected_tracks.contains(&&track.name))
        .collect();

    add_tracks_to_sink("media", Arc::clone(&sink), &tracks);

    // Send the new track names to all members, keep in mind they can contain unicode characters
    let track_names = tracks
        .iter()
        .map(|track| track.name.clone())
        .collect::<Vec<String>>()
        .join(",");
    let message = format!("tracks:{}", track_names);
    let member_list = members.lock().unwrap();
    for member in member_list.iter() {
        socket.send_to(message.as_bytes(), member)?;
    }

    let current_track_index = Arc::new(Mutex::new(0));
    let should_reset = Arc::new(Mutex::new(false));

    display_progress(
        Arc::clone(&sink),
        tracks.clone(),
        Arc::clone(&current_track_index),
        Arc::clone(&should_reset),
    );

    start_listener_thread(
        Arc::clone(&socket),
        Arc::clone(&sink),
        Arc::clone(&current_track_index),
        Arc::clone(&should_reset),
        tracks.clone(),
        Arc::clone(&members),
    );

    utils::start_track_position_thread(
        Arc::clone(&sink),
        Arc::clone(&current_track_index),
        Arc::clone(&should_reset),
        tracks.clone(),
    );

    user_input_loop(
        &socket,
        &sink,
        &current_track_index,
        &tracks,
        &should_reset,
        &members,
    )
}

/// Starts a background thread to broadcast ping messages and collect member responses.
///
/// This function continuously sends ping messages to a broadcast address to discover and register active members.
/// Each member's response is recorded in a shared `HashSet`. The thread stops when a termination signal is received.
fn start_ping_thread(
    socket: Arc<UdpSocket>,
    broadcast_addr: String,
    members: Arc<Mutex<HashSet<std::net::SocketAddr>>>,
    ping_thread_should_terminate: Arc<Mutex<bool>>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut broadcast_id = 0;
        loop {
            if *ping_thread_should_terminate.lock().unwrap() {
                break;
            }

            broadcast_id += 1;
            let ping_message = format!("PING,{}", broadcast_id);
            if let Err(e) = socket.send_to(ping_message.as_bytes(), &broadcast_addr) {
                eprintln!("Failed to send ping: {}", e);
            }

            socket
                .set_read_timeout(Some(Duration::from_millis(100)))
                .unwrap();

            loop {
                let mut buf = [0u8; 1024];
                match socket.recv_from(&mut buf) {
                    Ok((_, addr)) => {
                        let mut members = members.lock().unwrap();
                        if members.insert(addr) {
                            println!("Member count: {}", members.len());
                        }
                    }
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                    Err(e) => {
                        eprintln!("Failed to receive: {}", e);
                        break;
                    }
                }
            }

            std::thread::sleep(Duration::from_millis(500));
        }
    })
}

/// Stops the ping thread and notifies all members that broadcasting is complete.
///
/// This function terminates the ping thread by setting a shared flag and sending a termination message
/// to all registered members.
fn stop_ping_thread(
    ping_thread: std::thread::JoinHandle<()>,
    ping_thread_should_terminate: &Arc<Mutex<bool>>,
    socket: &Arc<UdpSocket>,
    members: &Arc<Mutex<HashSet<std::net::SocketAddr>>>,
) -> std::io::Result<()> {
    *ping_thread_should_terminate.lock().unwrap() = true;
    ping_thread.join().unwrap();
    let message = "Done broadcasting";
    for member in members.lock().unwrap().iter() {
        socket.send_to(message.as_bytes(), member)?;
    }
    Ok(())
}

/// Starts a background thread to listen for and handle incoming commands from members.
///
/// This function spawns a thread to receive commands from members via UDP and processes them.
/// Supported commands include playback control (`p`, `n`, `r`, `s`). The thread ensures synchronization
/// by broadcasting a global start time with each command.
fn start_listener_thread(
    socket: Arc<UdpSocket>,
    sink: Arc<Mutex<Sink>>,
    current_track_index: Arc<Mutex<usize>>,
    should_reset: Arc<Mutex<bool>>,
    tracks: Vec<Track>,
    members: Arc<Mutex<HashSet<std::net::SocketAddr>>>,
) {
    std::thread::spawn(move || {
        let mut buf = [0u8; 1024];
        loop {
            match socket.recv_from(&mut buf) {
                Ok((size, _addr)) => {
                    let message = String::from_utf8_lossy(&buf[..size]).to_string();
                    if message.starts_with("PING") {
                        continue; // Ignore PING messages
                    }

                    let global_start_time =
                        utils::broadcast_start_time().expect("Cannot obtain current time");
                    match message.trim() {
                        "p" => handle_command(
                            "0",
                            global_start_time,
                            &socket,
                            &sink,
                            &current_track_index,
                            &tracks,
                            &should_reset,
                            &members,
                        )
                        .unwrap(),
                        "n" => handle_command(
                            "2",
                            global_start_time,
                            &socket,
                            &sink,
                            &current_track_index,
                            &tracks,
                            &should_reset,
                            &members,
                        )
                        .unwrap(),
                        "s" => handle_command(
                            "3",
                            global_start_time,
                            &socket,
                            &sink,
                            &current_track_index,
                            &tracks,
                            &should_reset,
                            &members,
                        )
                        .unwrap(),
                        "r" => handle_command(
                            "4",
                            global_start_time,
                            &socket,
                            &sink,
                            &current_track_index,
                            &tracks,
                            &should_reset,
                            &members,
                        )
                        .unwrap(),
                        _ => println!("Unknown command from member: {}", message),
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(100)); // Optional sleep to reduce CPU usage
                }
                Err(e) => {
                    eprintln!("Error receiving from socket: {}", e);
                    break;
                }
            }
        }
    });
}

/// Handles user input to control playback and sends commands to all members.
///
/// This function continuously reads user input to process playback commands (`p`, `n`, `r`, `s`).
/// For each command, it broadcasts the command and a global start time to all members for synchronization.
fn user_input_loop(
    socket: &Arc<UdpSocket>,
    sink: &Arc<Mutex<Sink>>,
    current_track_index: &Arc<Mutex<usize>>,
    tracks: &Vec<Track>,
    should_reset: &Arc<Mutex<bool>>,
    members: &Arc<Mutex<HashSet<std::net::SocketAddr>>>,
) -> std::io::Result<()> {
    loop {
        let mut input = String::new();
        if std::io::stdin().read_line(&mut input).is_ok() {
            let global_start_time =
                utils::broadcast_start_time().expect("Cannot obtain current time");
            match input.trim() {
                "p" => handle_command(
                    "0",
                    global_start_time,
                    socket,
                    sink,
                    current_track_index,
                    tracks,
                    should_reset,
                    members,
                )?,
                "n" => handle_command(
                    "2",
                    global_start_time,
                    socket,
                    sink,
                    current_track_index,
                    tracks,
                    should_reset,
                    members,
                )?,
                "s" => handle_command(
                    "3",
                    global_start_time,
                    socket,
                    sink,
                    current_track_index,
                    tracks,
                    should_reset,
                    members,
                )?,
                "r" => handle_command(
                    "4",
                    global_start_time,
                    socket,
                    sink,
                    current_track_index,
                    tracks,
                    should_reset,
                    members,
                )?,
                _ => println!("Invalid command! Use 'p', 'n', 'r', or 's'."),
            }
        }
    }
}

/// Processes a playback command and broadcasts it to all members.
///
/// This function executes a playback command locally and synchronizes it across all members by broadcasting
/// the command and a global start time. Supported commands include:
/// - `"0"`: Play/Pause toggle.
/// - `"2"`: Skip to the next track.
/// - `"3"`: Stop playback and exit.
/// - `"4"`: Restart the current track.
fn handle_command(
    command: &str,
    global_start_time: u64,
    socket: &UdpSocket,
    sink: &Arc<Mutex<Sink>>,
    current_track_index: &Arc<Mutex<usize>>,
    tracks: &Vec<Track>,
    should_reset: &Arc<Mutex<bool>>,
    addr_list: &Arc<Mutex<HashSet<std::net::SocketAddr>>>,
) -> std::io::Result<()> {
    let message = format!("{} : {}", command, global_start_time);
    let addr_list = addr_list.lock().unwrap();
    for addr in addr_list.iter() {
        socket.send_to(message.as_bytes(), addr)?;
    }

    match command {
        "0" => utils::synchronized_action("p", global_start_time, sink),
        "2" => {
            {
                let mut track_index = current_track_index.lock().unwrap();
                *track_index += 1;
                if *track_index >= tracks.len() {
                    println!("\nNo more tracks!");
                    println!("Thanks for using the SyncStream!");
                    std::process::exit(0);
                }
            }
            utils::synchronized_action("n", global_start_time, sink);
            *should_reset.lock().unwrap() = true;
        }
        "3" => utils::synchronized_action("s", global_start_time, sink),
        "4" => utils::synchronized_action("r", global_start_time, sink),
        _ => {}
    }

    Ok(())
}
