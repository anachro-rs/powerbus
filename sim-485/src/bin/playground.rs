use std::num::NonZeroU8;
use std::thread::{spawn, sleep};
use std::time::{Duration, Instant};
use sim_485::*;

fn main() {
    let mut arc_bus = Rs485Bus::new_arc(10);

    let mut dev_1 = Rs485Device::new(&arc_bus, NonZeroU8::new(1).unwrap());
    let mut dev_2 = Rs485Device::new(&arc_bus, NonZeroU8::new(2).unwrap());
    let mut dev_3 = Rs485Device::new(&arc_bus, NonZeroU8::new(3).unwrap());

    let d1t = spawn(move || {
        let start = Instant::now();

        sleep(Duration::from_millis(1000));
        dev_1.enable_transmit();
        println!("Device 1 transmitting!");
        dev_1.send(&[1, 2, 3, 4]);
        dev_1.disable_transmit();
        dev_1.enable_listen();

        while start.elapsed() <= Duration::from_secs(10) {
            let data = dev_1.receive();
            if !data.is_empty() {
                println!("Device 1 heard: {:?}", data);
            }
            sleep(Duration::from_millis(10));
        }
    });

    let d2t = spawn(move || {
        let start = Instant::now();
        dev_2.enable_listen();

        while start.elapsed() <= Duration::from_secs(3) {
            let data = dev_2.receive();
            if !data.is_empty() {
                println!("Device 2 heard: {:?}", data);
            }
            sleep(Duration::from_millis(10));
        }

        dev_2.disable_listen();
        dev_2.enable_transmit();
        println!("Device 2 transmitting!");
        dev_2.send(&[5, 6, 7, 8]);
        dev_2.disable_transmit();
        dev_2.enable_listen();

        while start.elapsed() <= Duration::from_secs(9) {
            let data = dev_2.receive();
            if !data.is_empty() {
                println!("Device 2 heard: {:?}", data);
            }
            sleep(Duration::from_millis(10));
        }

        dev_2.disable_listen();
        dev_2.enable_transmit();
        println!("Device 2 transmitting (again)!");
        dev_2.send(&[5, 6, 7, 8]);
        dev_2.disable_transmit();
        dev_2.enable_listen();

        while start.elapsed() <= Duration::from_secs(10) {
            let data = dev_2.receive();
            if !data.is_empty() {
                println!("Device 2 heard: {:?}", data);
            }
            sleep(Duration::from_millis(10));
        }
    });

    let d3t = spawn(move || {
        let start = Instant::now();
        dev_3.enable_listen();

        while start.elapsed() <= Duration::from_secs(6) {
            let data = dev_3.receive();
            if !data.is_empty() {
                println!("Device 3 heard: {:?}", data);
            }
            sleep(Duration::from_millis(10));
        }

        dev_3.disable_listen();
        dev_3.enable_transmit();
        println!("Device 3 transmitting!");
        dev_3.send(&[9, 10, 11, 12]);
        // rut roh
        // dev_3.disable_transmit();
        dev_3.enable_listen();

        while start.elapsed() <= Duration::from_secs(10) {
            let data = dev_3.receive();
            if !data.is_empty() {
                println!("Device 3 heard: {:?}", data);
            }
            sleep(Duration::from_millis(10));
        }
    });

    d1t.join().unwrap();
    d2t.join().unwrap();
    d3t.join().unwrap();

}
