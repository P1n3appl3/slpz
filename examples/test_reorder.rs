fn main() {
    let events = [
        3, 1, 2, 3, 4, 5,
        1, 0, 1, 2, 
        1, 10, 11, 12, 
        2, 01,
        2, 02,
        3, 1, 2, 3, 4, 5,
        1, 20, 21, 22
    ];
    let mut event_sizes = [0u16; 256];
    event_sizes[1] = 3;
    event_sizes[2] = 1;
    event_sizes[3] = 5;

    let mut buf = Vec::new();
    slp_compress::reorder_events(&events, &event_sizes, &mut buf).unwrap();
    println!("{:?}", &buf);

    let mut buf2 = Vec::new();
    slp_compress::unorder_events(&buf, &event_sizes, &mut buf2).unwrap();

    assert_eq!(&orig, &buf2);
}
