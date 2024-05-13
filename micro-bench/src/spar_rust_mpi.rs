use std::fs::File;
use std::io::Write;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use spar_rust_v2::{mpi, to_stream};

#[derive(Debug, Serialize, Deserialize)]
struct Tcontent {
    line: i64,
    line_buffer: Vec<u8>,
    a_buffer: Vec<f64>,
    b_buffer: Vec<f64>,
    k_buffer: Vec<i32>,
}

#[spar_rust_v2::source]
fn source(size: usize) -> impl Iterator<Item = Tcontent> {
    (0..size).map(move |i| Tcontent {
        line: i as i64,
        line_buffer: vec![0; size],
        a_buffer: vec![0.0; size],
        b_buffer: vec![0.0; size],
        k_buffer: vec![0; size],
    })
}

#[spar_rust_v2::stage(State(iter_size1))]
fn stage1(mut content: Tcontent, iter_size1: i32) -> Tcontent {
    let size = content.line_buffer.len();
    let init_a = -2.125;
    let init_b = -1.5;
    let range = 3.0;
    let step = range / (size as f64);

    let im = init_b + (step * (content.line as f64));

    for j in 0..size {
        let mut a = init_a + step * j as f64;
        let cr = a;

        let mut b = im;
        let mut k = 0;

        for ii in 0..iter_size1 {
            let a2 = a * a;
            let b2 = b * b;
            if (a2 + b2) > 4.0 {
                break;
            }
            b = 2.0 * a * b + im;
            a = a2 - b2 + cr;
            k = ii;
        }
        content.a_buffer[j] = a;
        content.b_buffer[j] = b;
        content.k_buffer[j] = k;
    }
    content
}

#[spar_rust_v2::stage(State(iter_size1, iter_size2))]
fn stage2(mut content: Tcontent, iter_size1: i32, iter_size2: i32) -> Vec<u8> {
    let size = content.line_buffer.len();
    let init_a = -2.125;
    let init_b = -1.5;
    let range = 3.0;
    let step = range / (size as f64);

    let im = init_b + (step * (content.line as f64));

    for j in 0..size {
        let cr = init_a + step * j as f64;
        if content.k_buffer[j] == iter_size1 - 1 {
            for ii in iter_size1..iter_size1 + iter_size2 {
                let a2 = content.a_buffer[j] * content.a_buffer[j];
                let b2 = content.b_buffer[j] * content.b_buffer[j];
                if (a2 + b2) > 4.0 {
                    break;
                }
                content.b_buffer[j] = 2.0 * content.a_buffer[j] * content.b_buffer[j] + im;
                content.a_buffer[j] = a2 - b2 + cr;
                content.k_buffer[j] = ii;
            }
        }
        content.line_buffer[j] = (255.0
            - ((content.k_buffer[j] as f64) * 255.0 / ((iter_size1 + iter_size2) as f64)))
            as u8;
    }
    content.line_buffer
}

#[spar_rust_v2::sink(Ordered)]
fn sink(line_buffer: Vec<u8>) -> Vec<u8> {
    line_buffer
}

pub fn spar_rust_mpi_pipeline(size: usize, threads: usize, iter_size1: i32, iter_size2: i32) {
    let mut output = Vec::new();
    let start = SystemTime::now();

    for line_buffer in to_stream!(mpi: [
         source(size),
         (stage1(iter_size1), threads),
         (stage2(iter_size1, iter_size2), threads),
         sink,
    ])
    .0
    {
        output.extend(&line_buffer);
    }

    let system_duration = start.elapsed().expect("Failed to get render time?");
    let in_sec = system_duration.as_secs() as f64 + system_duration.subsec_nanos() as f64 * 1e-9;
    println!("Execution time spar-rust-mpi: {in_sec} sec");

    let mut buffer = File::create("result_spar-rust-mpi.txt").unwrap();
    buffer.write_all(&output).unwrap();
}
