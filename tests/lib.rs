extern crate swapper;

use std::thread;
use swapper::swapper;

#[test]
fn test() {
    let (us, them) = swapper();
    let helper = thread::spawn(move || {
        let mut hello = String::from("hello");
        them.swap(&mut hello).unwrap();
        assert_eq!(hello, "world");
    });
    let mut world = String::from("world");
    us.swap(&mut world).unwrap();
    assert_eq!(world, "hello");
    helper.join().unwrap();
}
