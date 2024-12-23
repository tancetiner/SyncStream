use rodio::{Decoder, Sink, Source};
use std::fs;
use std::io::{BufReader, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::track::Track;
use crate::utils::duration_to_minutes_seconds;

pub fn load_audio_files(media_dir: &str, tracks: &mut Vec<Track>) {
    let entries = fs::read_dir(media_dir).expect("Failed to read media directory");

    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            if let Some(extension) = path.extension() {
                if extension == "mp3" {
                    let track = create_track(&path);
                    tracks.push(track);
                }
            }
        }
    }

    tracks.sort();
}

/// Creates a Track data structure from the given path.
fn create_track(path: &std::path::Path) -> Track {
    let file_name = path.file_stem().unwrap().to_string_lossy().to_string();
    let file = BufReader::new(fs::File::open(&path).expect("Failed to open file"));
    let source = Decoder::new(file).expect("Failed to decode audio file");
    let duration = source.total_duration().expect("Failed to get duration");

    Track {
        name: file_name,
        duration,
    }
}

// function to add tracks to the sink
pub fn add_tracks_to_sink(media_dir: &str, sink: Arc<Mutex<Sink>>, tracks: &Vec<Track>) {
    for track in tracks.iter() {
        let path = format!("{}/{}.mp3", media_dir, track.name);
        let file = BufReader::new(fs::File::open(&path).expect("Failed to open file"));
        let source = Decoder::new(file).expect("Failed to decode audio file");
        sink.lock().unwrap().append(source);
    }

    print_playlist(tracks);
}

fn print_playlist(tracks: &[Track]) {
    println!("\nPlaylist:");
    for (i, track) in tracks.iter().enumerate() {
        println!(
            "\t{}: {} ({})",
            i + 1,
            track.name,
            duration_to_minutes_seconds(track.duration.as_secs())
        );
    }
    println!("\n");
}

/// Displays the progress of the current track in the sink.
pub fn display_progress(
    sink: Arc<Mutex<Sink>>,
    tracks: Vec<Track>,
    current_track_index: Arc<Mutex<usize>>,
    should_reset: Arc<Mutex<bool>>,
) {
    thread::spawn(move || loop {
        let track_index = *current_track_index.lock().unwrap();
        let track_name = &tracks[track_index].name;
        let track_duration = tracks[track_index].duration;

        loop {
            if *should_reset.lock().unwrap() {
                *should_reset.lock().unwrap() = false;
                break;
            }

            let position = sink.lock().unwrap().get_pos();
            display_progress_bar(&sink, track_name, track_duration, position);

            thread::sleep(Duration::from_millis(100));
        }
    });
}

/// Displays a progress bar for the current track in the Sink.
fn display_progress_bar(
    sink: &Arc<Mutex<Sink>>,
    track_name: &str,
    track_duration: Duration,
    position: Duration,
) {
    let bar_width = 50;
    let progress = position.as_secs_f64() / track_duration.as_secs_f64();
    let filled = (progress * bar_width as f64).round() as usize;
    let empty = bar_width - filled;

    print!(
        "\r{}: {} [{}{}] {} / {}\t",
        if sink.lock().unwrap().is_paused() {
            "Paused"
        } else {
            "Playing"
        },
        track_name,
        "=".repeat(filled),
        " ".repeat(empty),
        duration_to_minutes_seconds(position.as_secs()),
        duration_to_minutes_seconds(track_duration.as_secs()),
    );

    std::io::stdout().flush().unwrap();
}
