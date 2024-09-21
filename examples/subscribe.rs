use std::thread;
use std::time::Duration;

fn main() {
    darkmode::subscribe(|mode| println!("{mode:?}")).unwrap();
    thread::sleep(Duration::from_secs(u64::MAX));
}
