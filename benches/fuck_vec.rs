#![feature(new_zeroed_alloc, random)]

use criterion::{Criterion, criterion_group, criterion_main};
use std::alloc::{alloc, dealloc, Layout};
use std::hint::black_box;

fn criterion_benchmark(c: &mut Criterion) {
    const LEN: usize = 1_000_000;
    let mut buffer = [0u8; LEN];
    
    for i in 0..19 {
        buffer[i] = std::random::random::<u8>(..);

    };

    c.bench_function("vec", |b| b.iter(|| {
        let slice: Box<[u8]> = black_box(vec![0u8; LEN].into_boxed_slice());
        black_box(buffer.copy_from_slice(&slice));
    }));

    c.bench_function("zeroed", |b| b.iter(|| {
        let slice: Box<[u8]> = unsafe { black_box(Box::new_zeroed_slice(LEN).assume_init()) };
        black_box(buffer.copy_from_slice(&slice));
    }));

    c.bench_function("uninit", |b| b.iter(|| {
        let slice: Box<[u8]> = unsafe { black_box(Box::new_uninit_slice(LEN).assume_init()) };
        black_box(buffer.copy_from_slice(&slice));
    }));

    c.bench_function("alloc", |b| b.iter(|| { // fastests
        let ptr   = unsafe { alloc(Layout::array::<u8>(LEN).unwrap()) }; 
        let slice = unsafe { std::slice::from_raw_parts_mut(ptr, LEN) };

        black_box(buffer.copy_from_slice(&slice));

        unsafe { dealloc(ptr, Layout::array::<u8>(LEN).unwrap()); }        
    }));
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);