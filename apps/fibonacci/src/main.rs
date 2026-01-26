#![no_main]

pico_sdk::entrypoint!(main);

fn fibonacci(n: u32) -> u32 {
    let mut a = 0;
    let mut b = 1;
    for _ in 0..n {
        let sum = (a + b) % 7919;
        a = b;
        b = sum;
    }
    b
}

pub fn main() {
    let n = pico_sdk::io::read_as();
    let res = fibonacci(n);
    pico_sdk::io::commit(&res);
}
