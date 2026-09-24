use ropey::Rope;
use std::time::Instant;

#[test]
fn test_rope_large_file_creation() {
    // Create a 10 MB string
    let size = 10 * 1024 * 1024; // 10 MB
    let mut data = String::with_capacity(size);
    for _ in 0..size {
        data.push('a');
    }

    let start = Instant::now();
    let rope = Rope::from(&data);
    let duration = start.elapsed();

    println!("Time to create rope of {} MB: {:.2?}", size as f64 / 1024.0 / 1024.0, duration);
    assert_eq!(rope.len_bytes(), size as u64);
}

#[test]
fn test_rope_large_file_insertion() {
    // Start with a small rope
    let mut rope = Rope::from("Hello");
    let insert_str = "a".repeat(1000 * 1024); // 1 MB

    let start = Instant::now();
    rope.insert(5, &insert_str);
    let duration = start.elapsed();

    println!("Time to insert 1 MB into rope: {:.2?}", duration);
    assert_eq!(rope.len_bytes(), (5 + 1 + 1024 * 1024) as u64);
}

#[test]
fn test_rope_large_file_iteration() {
    // Create a 50 MB rope
    let size = 50 * 1024 * 1024; // 50 MB
    let data = "a".repeat(size);
    let rope = Rope::from(&data);

    let start = Instant::now();
    let mut count = 0;
    for ch in rope.chars() {
        count += 1;
    }
    let duration = start.elapsed();

    println!("Time to iterate over {} MB rope: {:.2?}", size as f64 / 1024.0 / 1024.0, duration);
    assert_eq!(count, size as u64);
}