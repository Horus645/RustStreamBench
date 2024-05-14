use std::cmp::Reverse;
use std::io::Write;
use std::time::SystemTime;
use std::{collections::BinaryHeap, fs::File};

use serde::{Deserialize, Serialize};
use spar_rust_v2::mpi::{
    self,
    traits::{Communicator, Destination, Source},
};

#[derive(Debug, Serialize, Deserialize)]
struct Tcontent {
    line: i64,
    line_buffer: Vec<u8>,
    a_buffer: Vec<f64>,
    b_buffer: Vec<f64>,
    k_buffer: Vec<i32>,
}

pub fn rsmpi_pipeline(size: usize, threads: usize, iter_size1: i32, iter_size2: i32) {
    let start = SystemTime::now();

    let (universe, _threading) =
        mpi::initialize_with_threading(mpi::Threading::Multiple).expect("failed to initialize mpi");
    let world = universe.world();
    let world_size = world.size() as usize;
    let threads = 1 + threads * 2;

    if world_size < threads {
        panic!("trying to execute with {threads} workers, but only have {size}");
    }

    let rank = world.rank();
    if rank as usize >= threads {
        std::process::exit(unsafe { mpi::ffi::MPI_Finalize() });
    }

    if rank == 0 {
        std::thread::spawn(move || {
            let mut sequence_number = 0u32;
            let comm = mpi::topology::SimpleCommunicator::world();
            let mut target_rank = 1;
            #[allow(clippy::explicit_counter_loop)]
            for i in 0..size {
                let content = Tcontent {
                    line: i as i64,
                    line_buffer: vec![0; size],
                    a_buffer: vec![0.0; size],
                    b_buffer: vec![0.0; size],
                    k_buffer: vec![0; size],
                };
                let bytes = bincode::serialize(&content).unwrap();
                let size = bytes.len() as u32;
                let target = comm.process_at_rank(target_rank);
                target.send(&size.to_ne_bytes());
                target.send(&bytes);
                target.send(&sequence_number);
                sequence_number += 1;

                target_rank += 1;
                if target_rank as usize >= threads / 2 {
                    target_rank = 1;
                }
            }
            for i in 1..(1 + threads / 2) {
                let target = comm.process_at_rank(i as i32);
                target.send(&0u32.to_ne_bytes());
            }
        });

        let mut output = Vec::new();
        let comm = world.any_process();
        let mut out_of_order = BinaryHeap::new();
        let mut cur_order = 0;
        for _ in 0..size {
            let (size, status) = comm.receive::<u32>();
            let mut buf = vec![0u8; size as usize];
            let status = world
                .process_at_rank(status.source_rank())
                .receive_into(&mut buf);
            let (sequence_number, _status) =
                world.process_at_rank(status.source_rank()).receive::<u32>();

            if cur_order == sequence_number {
                cur_order += 1;
                output.extend_from_slice(&buf[..]);
            } else {
                out_of_order.push(Reverse((sequence_number, buf)));
                while let Some(Reverse((i, b))) = out_of_order.pop() {
                    if i == cur_order {
                        cur_order += 1;
                        output.extend_from_slice(&b[..]);
                        continue;
                    }
                    out_of_order.push(Reverse((i, b)));
                    break;
                }
            }
        }
        while let Some(Reverse((_, b))) = out_of_order.pop() {
            output.extend_from_slice(&b[..]);
        }

        let system_duration = start.elapsed().expect("Failed to get render time?");
        let in_sec =
            system_duration.as_secs() as f64 + system_duration.subsec_nanos() as f64 * 1e-9;
        println!("Execution time mpi: {in_sec} sec");

        let mut buffer = File::create("result_mpi.txt").unwrap();
        buffer.write_all(&output).unwrap();
    } else if rank > 0 && rank as usize <= (threads / 2) {
        let begin = 1 + (threads / 2);
        let end = 2 * (threads / 2);
        let recver = world.any_process();
        let sender = mpi::topology::SimpleCommunicator::world();
        let mut target = (rank as usize % (1 + end - begin)) + begin;
        let mut zeros = 1;
        while zeros > 0 {
            let (size, status) = recver.receive::<u32>();
            if size == 0 {
                zeros -= 1;
                continue;
            }
            let mut buf = vec![0u8; size as usize];
            let _status = world
                .process_at_rank(status.source_rank())
                .receive_into(&mut buf);
            let (sequence_number, _status) =
                world.process_at_rank(status.source_rank()).receive::<u32>();
            let mut content: Tcontent = bincode::deserialize(&buf).unwrap();

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

            {
                let bytes = bincode::serialize(&content).unwrap();
                let size = bytes.len() as u32;
                let target = sender.process_at_rank(target as i32);
                target.send(&size.to_ne_bytes());
                target.send(&bytes);
                target.send(&sequence_number);
            }

            target += 1;
            if target > end {
                target = begin;
            }
        }

        for target in begin..(end + 1) {
            let target = sender.process_at_rank(target as i32);
            target.send(&0u32.to_ne_bytes());
        }
    } else {
        let recver = world.any_process();
        let sender = mpi::topology::SimpleCommunicator::world();
        let target = 0;
        let mut zeros = threads / 2;
        while zeros > 0 {
            let (size, status) = recver.receive::<u32>();
            if size == 0 {
                zeros -= 1;
                continue;
            }
            let mut buf = vec![0u8; size as usize];
            let _status = world
                .process_at_rank(status.source_rank())
                .receive_into(&mut buf);
            let (sequence_number, _status) =
                world.process_at_rank(status.source_rank()).receive::<u32>();
            let mut content: Tcontent = bincode::deserialize(&buf).unwrap();

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

            {
                let bytes = content.line_buffer;
                let size = bytes.len() as u32;
                let target = sender.process_at_rank(target);
                target.send(&size.to_ne_bytes());
                target.send(&bytes);
                target.send(&sequence_number);
            }
        }
    }
}
