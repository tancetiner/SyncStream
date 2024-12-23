use crate::track::Track;
use rodio::Sink;
use std::net::UdpSocket;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::UNIX_EPOCH;
use std::time::{Duration, SystemTime};

/// Starts a thread to monitor the current track's position and handle track transitions.
///
/// This function spawns a thread that continuously checks the playback position of the current track
/// using `Sink::get_pos()`. When the playback position exceeds the duration of the current track, the
/// thread advances to the next track by incrementing `current_track_index`, and signals a reset for
/// synchronization (setting `should_reset` to `true`). It checks if the `current_track_index` exceeds
/// the total number of tracks, and if there are no more tracks, the program prints a farewell message and exits.
pub fn start_track_position_thread(
    sink: Arc<Mutex<Sink>>,
    current_track_index: Arc<Mutex<usize>>,
    should_reset: Arc<Mutex<bool>>,
    tracks: Vec<Track>,
) {
    std::thread::spawn(move || loop {
        let track_index = *current_track_index.lock().unwrap();
        if sink.lock().unwrap().get_pos() >= tracks[track_index].duration {
            *current_track_index.lock().unwrap() += 1;
            *should_reset.lock().unwrap() = true;
            if *current_track_index.lock().unwrap() >= tracks.len() {
                println!("\nNo more tracks!");
                println!("Thanks for using the SyncStream!");
                std::process::exit(0);
            }
        }
    });
}

/// Converts a duration (in seconds) and returns a string formatted as `MM:SS`,
/// where `MM` represents the number of minutes and `SS` represents the
/// remaining seconds.
pub fn duration_to_minutes_seconds(seconds: u64) -> String {
    let minutes = seconds / 60;
    let seconds = seconds % 60;
    format!("{:02}:{:02}", minutes, seconds)
}

/// Calculates a start time 1 second in the future and returns it in milliseconds since the UNIX epoch.
///
/// The function uses a global clock for synchronization, ensuring aligned playback across devices
/// by obtaining the current time from an NTP server (e.g., Google's NTP service). This provides
/// accurate and consistent timing between devices.
///
/// In case the NTP request fails due to a network error, the function falls back to the system clock.
/// Using the system clock may introduce synchronization errors if the clocks on the devices
/// are not perfectly aligned.
pub fn broadcast_start_time() -> Option<u64> {
    let current_time_ms = match get_time_ms_ntp() {
        Ok(time) => time,
        Err(_) => {
            println!("Network Error! Using system time instead.");
            let current_time = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Failed to get current time");
            current_time.as_secs() * 1000 + current_time.subsec_millis() as u64
        }
    };

    let start_time_ms = current_time_ms + 1000;

    Some(start_time_ms)
}

/// Calculates the time offset until a given target time, returning the offset as a `Duration`.
///
/// The function determines the current time in milliseconds since the UNIX epoch using an NTP server
/// (e.g., Google's NTP service) to ensure accurate synchronization. The offset is calculated by
/// comparing the current time with the provided `target_time_ms`.
///
/// If the current time is already past the target time, the function returns a `Duration` of zero.
/// In case of a failure to obtain the current time via NTP, the function falls back to the system clock, 
/// but this might lead to desynchronization between devices if the system clocks are not aligned.
fn get_offset(target_time_ms: u64) -> Option<Duration> {
    let current_time_ms = match get_time_ms_ntp() {
        Ok(time) => time,
        Err(_) => {
            println!("Network Error! Using system time instead.");
            let current_time = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Failed to get current time");
            current_time.as_secs() * 1000 + current_time.subsec_millis() as u64
        }
    };

    let current_time = Duration::from_millis(current_time_ms);
    let target_time = Duration::from_millis(target_time_ms);

    if current_time < target_time {
        Some(target_time - current_time)
    } else {
        println!("Not enough time to synchronize!");
        Some(Duration::from_secs(0)) // Time already passed
    }
}

/// Executes a synchronized action at the specified target time based on the provided role.
///
/// The function waits until the offset duration (calculated as the difference between the current time
/// and the target time) has elapsed.
///
/// The specific action performed depends on the given `role` parameter:
///   - "p": Toggles playback (play/pause) of the audio sink.
///   - "n": Skips to the next track in the audio sink.
///   - "s": Stops the application with a goodbye message.
///   - "r": Restarts the currently playing track from the beginning.
pub fn synchronized_action(role: &str, target_time_ms: u64, sink_clone: &Arc<Mutex<Sink>>) {
    let offset = get_offset(target_time_ms).expect("Cannot obtain offset");

    thread::sleep(offset);

    // Execute the action at the target time
    match role.trim() {
        "p" => {
            let sink = sink_clone.lock().unwrap();
            if sink.is_paused() {
                sink.play();
            } else {
                sink.pause();
            }
        }
        "n" => {
            sink_clone.lock().unwrap().skip_one();
        }
        "s" => {
            println!("\nThanks for using the SyncStream!");
            std::process::exit(0);
        }
        "r" => {
            sink_clone
                .lock()
                .unwrap()
                .try_seek(Duration::from_secs(0))
                .expect("Cannot restart the track");
        }
        _ => {}
    }
}

/// Retrieves the current time in milliseconds since the UNIX epoch using an NTP server.
///
/// This function establishes a UDP connection to an NTP server (e.g., Google's NTP service) and
/// fetches the current time. The time is returned with millisecond precision by combining the whole
/// seconds and fractional seconds obtained from the NTP response.
///
/// The NTP server used is `time.google.com:123`. This can be replaced with any other valid NTP server.
/// The function sets a 2-second timeout for the UDP socket. If no response is received within this time,
/// the operation fails with a timeout error.
pub fn get_time_ms_ntp() -> Result<u64, sntpc::Error> {
    let socket = UdpSocket::bind("0.0.0.0:0").expect("Unable to crate UDP socket");
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("Unable to set UDP socket read timeout");

    let result = sntpc::simple_get_time("time.google.com:123", &socket)?;

    let seconds = result.sec() as u64; // Whole seconds
    let millis = sntpc::fraction_to_milliseconds(result.sec_fraction()); // Fractional part in milliseconds

    // Combine seconds and milliseconds into total milliseconds
    let current_time_ms = seconds * 1000 + millis as u64;

    Ok(current_time_ms)
}

/// Extracts a timestamp from a colon-delimited input string.
///
/// This function splits the input string at the first colon (`:`) and parses the
/// second part (after the colon) as an unsigned 64-bit integer (`u64`). The parsed value
/// represents the extracted timestamp.
pub fn extract_timestamp(input: &str) -> Option<u64> {
    // Split the string to find the timestamp
    if let Some(number_str) = input.split(':').nth(1) {
        // Trim whitespace and parse the number
        number_str.trim().parse::<u64>().ok()
    } else {
        None
    }
}

/// Extracts a mode value from a colon-delimited input string.
///
/// This function splits the input string at the first colon (`:`) and parsea the
/// first part (before the colon) as an unsigned 64-bit integer (`u64`). The parsed value
/// represents the extracted mode.
pub fn extract_mode(input: &str) -> Option<u64> {
    // Split the string to find the mode (0/2/3)
    if let Some(number_str) = input.split(':').nth(0) {
        // Trim whitespace and parse the number
        number_str.trim().parse::<u64>().ok()
    } else {
        None
    }
}

// Unit testing
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_broadcast_start_time() {
        let start_time = broadcast_start_time().expect("Expected valid start time");

        let current_time_ms = match get_time_ms_ntp() {
            Ok(time) => time,
            Err(_) => {
                println!("Network Error! Using system time instead.");
                let current_time = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("Failed to get current time");
                current_time.as_secs() * 1000 + current_time.subsec_millis() as u64
            }
        };

        // Ensure the start time is at least 500ms in the future
        assert!(
            start_time >= current_time_ms + 500,
            "Start time is too early"
        );
    }

    #[test]
    fn test_get_offset_future_time() {
        let current_time_ms = match get_time_ms_ntp() {
            Ok(time) => time,
            Err(_) => {
                println!("Network Error! Using system time instead.");
                let current_time = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("Failed to get current time");
                current_time.as_secs() * 1000 + current_time.subsec_millis() as u64
            }
        };

        let target_time_ms = current_time_ms + 1000; // 1 second into the future
        let offset = get_offset(target_time_ms).expect("Expected valid offset");

        // Offset should be close to 1 second
        assert!(offset >= Duration::from_millis(900) && offset <= Duration::from_millis(1100));
    }

    #[test]
    fn test_get_offset_past_time() {
        let current_time_ms = match get_time_ms_ntp() {
        Ok(time) => time,
        Err(_) => {
            println!("Network Error! Using system time instead.");
            let current_time = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Failed to get current time");
            current_time.as_secs() * 1000 + current_time.subsec_millis() as u64
        }
    };

        let target_time_ms = current_time_ms - 1000; // 1 second in the past
        let offset = get_offset(target_time_ms).expect("Expected valid offset");

        // Offset should be 0 as the time has already passed
        assert_eq!(offset, Duration::from_secs(0));
    }

    #[test]
    fn test_extract_timestamp_valid() {
        let input = "timestamp:1234567890";
        let timestamp = extract_timestamp(input).expect("Expected valid timestamp");

        assert_eq!(timestamp, 1234567890, "Timestamp extraction failed");
    }

    #[test]
    fn test_extract_timestamp_invalid() {
        let input = "timestamp:";
        let timestamp = extract_timestamp(input);

        assert!(timestamp.is_none(), "Expected None for invalid timestamp");
    }

    #[test]
    fn test_extract_mode_valid() {
        let input = "1:timestamp";
        let mode = extract_mode(input).expect("Expected valid mode");

        assert_eq!(mode, 1, "Mode extraction failed");
    }

    #[test]
    fn test_extract_mode_invalid() {
        let input = ":timestamp";
        let mode = extract_mode(input);

        assert!(mode.is_none(), "Expected None for invalid mode");
    }
}
