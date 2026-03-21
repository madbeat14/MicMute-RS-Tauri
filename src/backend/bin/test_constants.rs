use windows::Win32::Media::Audio::{eCapture, eRender, eConsole, eMultimedia, eCommunications};

fn main() {
    println!("eCapture: {:?}", eCapture);
    println!("eRender: {:?}", eRender);
    println!("eConsole: {:?}", eConsole);
    println!("eMultimedia: {:?}", eMultimedia);
    println!("eCommunications: {:?}", eCommunications);
}
