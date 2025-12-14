use std::process::exit;
use std::thread;
use std::time::Duration;

use anyhow::Result;
use clap::Parser;

mod bitalino;
mod bluetooth;
mod errors;

#[derive(Parser, Debug)]
#[command(name = "bitalino-demo", about = "Connect to BITalino and read frames")]
struct Args {
    /// Bluetooth MAC address (e.g., 20:16:10:XX:XX:XX)
    mac: String,
    /// Pairing PIN code (e.g., 1234)
    pin: String,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {e}");
        exit(1);
    }
}

fn run() -> Result<()> {
    let args = Args::parse();

    println!("Using MAC: {}, PIN: {}", args.mac, args.pin);
    println!("--- Initializing Bluetooth Sensor (Rust) ---");
    let connector = bluetooth::BluetoothConnector::default();
    let stream = connector.pair_and_connect(&args.mac, &args.pin)?;

    // 2. Connection
    let mut device = bitalino::Bitalino::from_rfcomm(stream);

    println!("Connected! Getting Version...");
    match device.version() {
        Ok(v) => println!("Version: {}", v.trim()),
        Err(e) => println!("Version: Unknown ({e})"),
    }

    // 3. Acquisition
    println!("Starting Acquisition (1000Hz)...");
    device.start(1000, vec![0, 1, 2, 3, 4, 5])?;

    println!("Reading 10 batches of 100 samples...");
    for i in 0..10 {
        match device.read_frames(100) {
            Ok(frames) => {
                if let Some(first) = frames.first() {
                    println!(
                        "[Batch {}] Seq: {:02} | Analog: {:?}",
                        i, first.seq, first.analog
                    );
                }
            }
            Err(e) => eprintln!("Read error: {}", e),
        }
        thread::sleep(Duration::from_millis(10));
    }

    // 4. Cleanup
    println!("Stopping...");
    device.stop()?;
    println!("Done.");
    Ok(())
}
