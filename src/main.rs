mod leader;
mod member;
mod player;
mod track;
mod utils;

use asky::Select;

fn main() -> std::io::Result<()> {
    println!("Welcome to SyncStream!");
    let options = ["Leader (Playback Controller)", "Member (Music Enjoyer)"];
    let answer = Select::new("Which role do you want?", options).prompt()?;

    match answer {
        "Leader (Playback Controller)" => leader::run_leader()?,
        "Member (Music Enjoyer)" => member::run_member()?,
        _ => {}
    }

    Ok(())
}
