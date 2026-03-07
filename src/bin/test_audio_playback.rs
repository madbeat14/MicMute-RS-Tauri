use rodio::{OutputStream, Sink};
use std::fs::File;
use std::io::BufReader;

fn main() {
    let (_stream, stream_handle) = OutputStream::try_default().unwrap();
    let sink = Sink::try_new(&stream_handle).unwrap();

    let paths = vec!["assets/mute.wav", "assets/unmute.wav"];

    for path in paths {
        println!("Testing path: {}", path);
        if let Ok(file) = File::open(path) {
            println!("  File opened successfully.");
            match rodio::Decoder::new(BufReader::new(file)) {
                Ok(source) => {
                    println!("  Decoder created successfully.");
                    sink.append(source);
                    sink.sleep_until_end();
                    println!("  Playback finished.");
                }
                Err(e) => {
                    println!("  Decoder failed: {:?}", e);
                }
            }
        } else {
            println!("  File NOT found at {}", path);
        }
    }
}
