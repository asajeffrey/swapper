# swapper
Swap ownership of data between threads in Rust.

This crate allows threads to swap ownership of data without a transitory state where the thread owns nothing.

```rust,skt-main
   let (ab, ba) = swapper::swapper();
   thread::spawn(move || {
      let mut a = String::from("hello");
      ab.swap(&mut a).unwrap();
      assert_eq!(a, "world");
   });
   let mut b = String::from("world");
   ba.swap(&mut b).unwrap();
   assert_eq!(b, "hello");
```
