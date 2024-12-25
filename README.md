# SyncStream

## Description
Welcome to SyncStream! SyncStream is a real-time media synchronization tool that enables multiple participants to experience music or audio tracks simultaneously. Designed with a “leader-member” architecture, a client-server model rebranded to enhance user experience. SyncStream synchronizes playback commands like "play, pause, restart, stop, and skip" across participants connected to the same network. By leveraging UDP for lightweight communication, SyncStream ensures minimal latency during synchronization.

## Authors and acknowledgment
Developed by [Tan Cetiner](https://github.com/tancetiner) and [Brian Ooi](https://github.com/brianooi0504) as the term project for the "NET7212 - Safe System Programming with Rust" course.
The project was initially developed in a private GitLab repository during the course and later uploaded here, which is why it does not contain the commit history. I tried using the “import project” feature of GitHub, but for some reason, it repeatedly failed—looking at you, Microsoft…

## Features
-   Real-time playback synchronization across devices.
-	Both the leader and members can issue commands, which are broadcasted to all participants for synchronized execution.
    - Supported commands: play, pause, restart, stop, and skip.
    - Commands are processed in real-time during playback.
- Dynamic participant discovery through UDP broadcasting.
- Multi-track support with detailed progress display:
    - Includes track name, current time, total duration, and a progress bar.
-   Lightweight and cross-platform.

## Installation
-   Rust programming language and Cargo package manager
-   Media files are stored in the “media” folder, with support currently limited to MP3 files.


1. Clone the repository `$ git clone {project_url} -o syncstream`
2. Go to the project directory `$ cd syncstream`
3. Create a "media" directory `$ mkdir media`
4. Place some MP3 files inside `$ cp ~/my_cool_media_file.mp3 ./media`
5. Build and run! `$ cargo run`

## Usage
Once the application is started, roles can be selected. It is expected that there is one leader who synchronizes the playing of all the other members. 

The leader or members can then enter:
-   'p' to play or pause the music
-   'n' for next track
-   'r' for restarting the track
-   's' for stopping the playback and quit the program

## Future work
The time constraints and scope of the project prevented us from implementing every feature we had envisioned. Here are some of them. If we can find spare time, we would like to continue working on these:
-   Playlist Selection: Before starting the playback, the leader can select which music files are included in the playing session.
-   Volume Sync: Allow volume adjustments synchronized across all members.
-   Streaming Support: Enable the leader to stream music files directly to members instead of requiring local files. This feature will:
    -   Support on-the-fly music streaming over the network.
    -   Reduce setup complexity for members by eliminating the need to have local media files.
