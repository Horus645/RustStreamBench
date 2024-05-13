use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::fs::File;
use std::io::{prelude::*, BufWriter};
use std::mem;
use std::time::SystemTime;

use spar_rust_v2::mpi::{
    self,
    traits::{Communicator, Destination, Source},
};

use crate::BLOCK_SIZE;

pub fn rsmpi(threads: usize, file_action: &str, file_name: &str) {
    let mut file = File::open(file_name).expect("No file found.");
    let mut buffer_input = Vec::with_capacity(1 << 10);
    file.read_to_end(&mut buffer_input).unwrap();
    let threads = threads + 1;

    if file_action == "compress" {
        let compressed_file_name = file_name.to_owned() + ".bz2";
        let outfile = File::create(compressed_file_name).unwrap();
        let mut buf_write = BufWriter::new(outfile);

        let start = SystemTime::now();

        let (universe, _threading) = mpi::initialize_with_threading(mpi::Threading::Multiple)
            .expect("failed to initialize mpi");
        let world = universe.world();
        let size = world.size() as usize;

        if size < threads {
            panic!("trying to execute with {threads} workers, but only have {size}");
        }

        let rank = world.rank();
        if rank as usize >= threads {
            std::process::exit(unsafe { mpi::ffi::MPI_Finalize() });
        }

        if rank == 0 {
            let mut sequences = (buffer_input.len() + BLOCK_SIZE - 1) / BLOCK_SIZE;
            std::thread::spawn(move || {
                let comm = mpi::topology::SimpleCommunicator::world();
                let mut target_rank = 1;
                let mut sequence_number = 0u32;
                let mut pos_init: usize;
                let mut pos_end = 0;
                let mut bytes_left = buffer_input.len();

                while bytes_left > 0 {
                    pos_init = pos_end;
                    pos_end += if bytes_left < BLOCK_SIZE {
                        buffer_input.len() - pos_end
                    } else {
                        BLOCK_SIZE
                    };
                    bytes_left -= pos_end - pos_init;

                    let buffer_slice = &buffer_input[pos_init..pos_end];
                    let size = buffer_slice.len() as u32;
                    let target = comm.process_at_rank(target_rank);

                    target.send(&size.to_ne_bytes());
                    target.send(buffer_slice);
                    target.send(&sequence_number.to_ne_bytes());

                    sequence_number += 1;
                    target_rank += 1;
                    if target_rank as usize == threads {
                        target_rank = 1;
                    }
                }

                for i in 1..threads {
                    let target = comm.process_at_rank(i as i32);
                    target.send(&0u32.to_ne_bytes());
                }
            });

            let mut output = Vec::new();

            let comm = world.any_process();
            let mut out_of_order = BinaryHeap::new();
            let mut cur_order = 0;
            while sequences > 0 {
                sequences -= 1;
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
            println!("Execution time: {in_sec} sec");

            // write compressed data to file
            buf_write.write_all(&output).unwrap();
        } else {
            loop {
                let comm = world.process_at_rank(0);
                let (size, status) = comm.receive::<u32>();
                if size == 0 {
                    break;
                }
                let mut buf = vec![0u8; size as usize];
                let status = world
                    .process_at_rank(status.source_rank())
                    .receive_into(&mut buf);
                let (sequence_number, _status) =
                    world.process_at_rank(status.source_rank()).receive::<u32>();
                unsafe {
                    let mut bz_buffer: bzip2_sys::bz_stream = mem::zeroed();
                    bzip2_sys::BZ2_bzCompressInit(&mut bz_buffer as *mut _, 9, 0, 30);

                    let mut output: Vec<u8> = vec![0; (buf.len() as f64 * 1.01) as usize + 600];

                    bz_buffer.next_in = buf.as_ptr() as *mut _;
                    bz_buffer.avail_in = buf.len() as _;
                    bz_buffer.next_out = output.as_mut_ptr() as *mut _;
                    bz_buffer.avail_out = output.len() as _;

                    bzip2_sys::BZ2_bzCompress(&mut bz_buffer as *mut _, bzip2_sys::BZ_FINISH as _);
                    bzip2_sys::BZ2_bzCompressEnd(&mut bz_buffer as *mut _);

                    comm.send(&bz_buffer.total_out_lo32.to_ne_bytes());
                    comm.send(&output[0..bz_buffer.total_out_lo32 as usize]);
                    comm.send(&sequence_number.to_ne_bytes());
                }
            }
        }
    } else if file_action == "decompress" {
        // creating the decompressed file
        let decompressed_file_name = &file_name.to_owned()[..file_name.len() - 4];
        let outfile = File::create(decompressed_file_name).unwrap();
        let mut buf_write = BufWriter::new(outfile);

        // initialization
        let mut pos_init: usize;
        let mut pos_end = 0;
        let mut bytes_left = buffer_input.len();
        let mut queue_blocks: Vec<(usize, usize)> = Vec::new();

        while bytes_left > 0 {
            pos_init = pos_end;
            pos_end += {
                // find the ending position by identifing the header of the next stream block
                let buffer_slice;
                if buffer_input.len() > BLOCK_SIZE + 10000 {
                    if (pos_init + BLOCK_SIZE + 10000) > buffer_input.len() {
                        buffer_slice = &buffer_input[pos_init + 10..];
                    } else {
                        buffer_slice = &buffer_input[pos_init + 10..pos_init + BLOCK_SIZE + 10000];
                    }
                } else {
                    buffer_slice = &buffer_input[pos_init + 10..];
                }

                let ret = buffer_slice
                    .windows(10)
                    .position(|window| window == b"BZh91AY&SY");
                match ret {
                    Some(i) => i + 10,
                    None => buffer_input.len() - pos_init,
                }
            };
            bytes_left -= pos_end - pos_init;
            queue_blocks.push((pos_init, pos_end));
        }

        let start = SystemTime::now();

        let (universe, _threading) = mpi::initialize_with_threading(mpi::Threading::Multiple)
            .expect("failed to initialize mpi");
        let world = universe.world();
        let size = world.size() as usize;

        if size < threads {
            panic!("trying to execute with {threads} workers, but only have {size}");
        }

        let rank = world.rank();
        if rank as usize >= threads {
            std::process::exit(unsafe { mpi::ffi::MPI_Finalize() });
        }

        if rank == 0 {
            let mut sequences = queue_blocks.len();
            std::thread::spawn(move || {
                let comm = mpi::topology::SimpleCommunicator::world();
                let mut target_rank = 1;

                for (sequence_number, block) in queue_blocks.into_iter().enumerate() {
                    let buffer_slice = &buffer_input[block.0..block.1];
                    let size = buffer_slice.len() as u32;
                    let target = comm.process_at_rank(target_rank);

                    target.send(&size.to_ne_bytes());
                    target.send(buffer_slice);
                    target.send(&(sequence_number as u32).to_ne_bytes());

                    target_rank += 1;
                    if target_rank as usize == threads {
                        target_rank = 1;
                    }
                }

                for i in 1..threads {
                    let target = comm.process_at_rank(i as i32);
                    target.send(&0u32.to_ne_bytes());
                }
            });

            let mut output = Vec::new();

            let comm = world.any_process();
            let mut out_of_order = BinaryHeap::new();
            let mut cur_order = 0;
            while sequences > 0 {
                sequences -= 1;
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
            println!("Execution time: {in_sec} sec");

            // write compressed data to file
            buf_write.write_all(&output).unwrap();
        } else {
            loop {
                let comm = world.process_at_rank(0);
                let (size, status) = comm.receive::<u32>();
                if size == 0 {
                    break;
                }
                let mut buf = vec![0u8; size as usize];
                let status = world
                    .process_at_rank(status.source_rank())
                    .receive_into(&mut buf);
                let (sequence_number, _status) =
                    world.process_at_rank(status.source_rank()).receive::<u32>();
                unsafe {
                    let mut bz_buffer: bzip2_sys::bz_stream = mem::zeroed();
                    bzip2_sys::BZ2_bzDecompressInit(&mut bz_buffer as *mut _, 0, 0);

                    let mut output: Vec<u8> = vec![0; BLOCK_SIZE];

                    bz_buffer.next_in = buf.as_ptr() as *mut _;
                    bz_buffer.avail_in = buf.len() as _;
                    bz_buffer.next_out = output.as_mut_ptr() as *mut _;
                    bz_buffer.avail_out = output.len() as _;

                    bzip2_sys::BZ2_bzDecompress(&mut bz_buffer as *mut _);
                    bzip2_sys::BZ2_bzDecompressEnd(&mut bz_buffer as *mut _);

                    comm.send(&bz_buffer.total_out_lo32.to_ne_bytes());
                    comm.send(&output[0..bz_buffer.total_out_lo32 as usize]);
                    comm.send(&sequence_number.to_ne_bytes());
                }
            }
        }
    }
}

pub fn rsmpi_io(threads: usize, file_action: &str, file_name: &str) {
    let mut file = File::open(file_name).expect("No file found.");
    let mut buffer_input = Vec::with_capacity(1 << 10);
    file.read_to_end(&mut buffer_input).unwrap();
    let threads = threads + 1;

    if file_action == "compress" {
        let compressed_file_name = file_name.to_owned() + ".bz2";

        // initialization
        let start = SystemTime::now();

        let (universe, _threading) = mpi::initialize_with_threading(mpi::Threading::Multiple)
            .expect("failed to initialize mpi");
        let world = universe.world();
        let size = world.size() as usize;

        if size < threads {
            panic!("trying to execute with {threads} workers, but only have {size}");
        }

        let rank = world.rank();
        if rank as usize >= threads {
            std::process::exit(unsafe { mpi::ffi::MPI_Finalize() });
        }

        if rank == 0 {
            let mut sequences = (buffer_input.len() + BLOCK_SIZE - 1) / BLOCK_SIZE;
            std::thread::spawn(move || {
                let comm = mpi::topology::SimpleCommunicator::world();
                let mut target_rank = 1;
                let mut sequence_number = 0u32;
                let mut pos_init: usize;
                let mut pos_end = 0;
                let mut bytes_left = buffer_input.len();

                while bytes_left > 0 {
                    pos_init = pos_end;
                    pos_end += if bytes_left < BLOCK_SIZE {
                        buffer_input.len() - pos_end
                    } else {
                        BLOCK_SIZE
                    };
                    bytes_left -= pos_end - pos_init;

                    let buffer_slice = &buffer_input[pos_init..pos_end];
                    let size = buffer_slice.len() as u32;
                    let target = comm.process_at_rank(target_rank);

                    target.send(&size.to_ne_bytes());
                    target.send(buffer_slice);
                    target.send(&sequence_number.to_ne_bytes());

                    sequence_number += 1;
                    target_rank += 1;
                    if target_rank as usize == threads {
                        target_rank = 1;
                    }
                }

                for i in 1..threads {
                    let target = comm.process_at_rank(i as i32);
                    target.send(&0u32.to_ne_bytes());
                }
            });

            let outfile = File::create(compressed_file_name).unwrap();
            let mut buf_write = BufWriter::new(outfile);

            let comm = world.any_process();
            let mut out_of_order = BinaryHeap::new();
            let mut cur_order = 0;
            while sequences > 0 {
                sequences -= 1;
                let (size, status) = comm.receive::<u32>();
                let mut buf = vec![0u8; size as usize];
                let status = world
                    .process_at_rank(status.source_rank())
                    .receive_into(&mut buf);
                let (sequence_number, _status) =
                    world.process_at_rank(status.source_rank()).receive::<u32>();

                if cur_order == sequence_number {
                    cur_order += 1;
                    buf_write.write_all(&buf[..]).unwrap();
                } else {
                    out_of_order.push(Reverse((sequence_number, buf)));
                    while let Some(Reverse((i, b))) = out_of_order.pop() {
                        if i == cur_order {
                            cur_order += 1;
                            buf_write.write_all(&b[..]).unwrap();
                            continue;
                        }
                        out_of_order.push(Reverse((i, b)));
                        break;
                    }
                }
            }
            while let Some(Reverse((_, b))) = out_of_order.pop() {
                buf_write.write_all(&b[..]).unwrap();
            }
            let system_duration = start.elapsed().expect("Failed to get render time?");
            let in_sec =
                system_duration.as_secs() as f64 + system_duration.subsec_nanos() as f64 * 1e-9;
            println!("Execution time: {in_sec} sec");
        } else {
            loop {
                let comm = world.process_at_rank(0);
                let (size, status) = comm.receive::<u32>();
                if size == 0 {
                    break;
                }
                let mut buf = vec![0u8; size as usize];
                let status = world
                    .process_at_rank(status.source_rank())
                    .receive_into(&mut buf);
                let (sequence_number, _status) =
                    world.process_at_rank(status.source_rank()).receive::<u32>();
                unsafe {
                    let mut bz_buffer: bzip2_sys::bz_stream = mem::zeroed();
                    bzip2_sys::BZ2_bzCompressInit(&mut bz_buffer as *mut _, 9, 0, 30);

                    let mut output: Vec<u8> = vec![0; (buf.len() as f64 * 1.01) as usize + 600];

                    bz_buffer.next_in = buf.as_ptr() as *mut _;
                    bz_buffer.avail_in = buf.len() as _;
                    bz_buffer.next_out = output.as_mut_ptr() as *mut _;
                    bz_buffer.avail_out = output.len() as _;

                    bzip2_sys::BZ2_bzCompress(&mut bz_buffer as *mut _, bzip2_sys::BZ_FINISH as _);
                    bzip2_sys::BZ2_bzCompressEnd(&mut bz_buffer as *mut _);

                    comm.send(&bz_buffer.total_out_lo32.to_ne_bytes());
                    comm.send(&output[0..bz_buffer.total_out_lo32 as usize]);
                    comm.send(&sequence_number.to_ne_bytes());
                }
            }
        }
    } else if file_action == "decompress" {
        let decompressed_file_name = &file_name.to_owned()[..file_name.len() - 4];

        // initialization
        let mut pos_init: usize;
        let mut pos_end = 0;
        let mut bytes_left = buffer_input.len();
        let mut queue_blocks: Vec<(usize, usize)> = Vec::new();

        while bytes_left > 0 {
            pos_init = pos_end;
            pos_end += {
                // find the ending position by identifing the header of the next stream block
                let buffer_slice;
                if buffer_input.len() > BLOCK_SIZE + 10000 {
                    if (pos_init + BLOCK_SIZE + 10000) > buffer_input.len() {
                        buffer_slice = &buffer_input[pos_init + 10..];
                    } else {
                        buffer_slice = &buffer_input[pos_init + 10..pos_init + BLOCK_SIZE + 10000];
                    }
                } else {
                    buffer_slice = &buffer_input[pos_init + 10..];
                }

                let ret = buffer_slice
                    .windows(10)
                    .position(|window| window == b"BZh91AY&SY");
                match ret {
                    Some(i) => i + 10,
                    None => buffer_input.len() - pos_init,
                }
            };
            bytes_left -= pos_end - pos_init;
            queue_blocks.push((pos_init, pos_end));
        }

        let start = SystemTime::now();

        let (universe, _threading) = mpi::initialize_with_threading(mpi::Threading::Multiple)
            .expect("failed to initialize mpi");
        let world = universe.world();
        let size = world.size() as usize;

        if size < threads {
            panic!("trying to execute with {threads} workers, but only have {size}");
        }

        let rank = world.rank();
        if rank as usize >= threads {
            std::process::exit(unsafe { mpi::ffi::MPI_Finalize() });
        }

        if rank == 0 {
            let mut sequences = queue_blocks.len();
            std::thread::spawn(move || {
                let comm = mpi::topology::SimpleCommunicator::world();
                let mut target_rank = 1;

                for (sequence_number, block) in queue_blocks.into_iter().enumerate() {
                    let buffer_slice = &buffer_input[block.0..block.1];
                    let size = buffer_slice.len() as u32;
                    let target = comm.process_at_rank(target_rank);

                    target.send(&size.to_ne_bytes());
                    target.send(buffer_slice);
                    target.send(&(sequence_number as u32).to_ne_bytes());

                    target_rank += 1;
                    if target_rank as usize == threads {
                        target_rank = 1;
                    }
                }

                for i in 1..threads {
                    let target = comm.process_at_rank(i as i32);
                    target.send(&0u32.to_ne_bytes());
                }
            });

            let outfile = File::create(decompressed_file_name).unwrap();
            let mut buf_write = BufWriter::new(outfile);

            let comm = world.any_process();
            let mut out_of_order = BinaryHeap::new();
            let mut cur_order = 0;
            while sequences > 0 {
                sequences -= 1;
                let (size, status) = comm.receive::<u32>();
                let mut buf = vec![0u8; size as usize];
                let status = world
                    .process_at_rank(status.source_rank())
                    .receive_into(&mut buf);
                let (sequence_number, _status) =
                    world.process_at_rank(status.source_rank()).receive::<u32>();

                if cur_order == sequence_number {
                    cur_order += 1;
                    buf_write.write_all(&buf[..]).unwrap();
                } else {
                    out_of_order.push(Reverse((sequence_number, buf)));
                    while let Some(Reverse((i, b))) = out_of_order.pop() {
                        if i == cur_order {
                            cur_order += 1;
                            buf_write.write_all(&b[..]).unwrap();
                            continue;
                        }
                        out_of_order.push(Reverse((i, b)));
                        break;
                    }
                }
            }
            while let Some(Reverse((_, b))) = out_of_order.pop() {
                buf_write.write_all(&b[..]).unwrap();
            }
            let system_duration = start.elapsed().expect("Failed to get render time?");
            let in_sec =
                system_duration.as_secs() as f64 + system_duration.subsec_nanos() as f64 * 1e-9;
            println!("Execution time: {in_sec} sec");
        } else {
            loop {
                let comm = world.process_at_rank(0);
                let (size, status) = comm.receive::<u32>();
                if size == 0 {
                    break;
                }
                let mut buf = vec![0u8; size as usize];
                let status = world
                    .process_at_rank(status.source_rank())
                    .receive_into(&mut buf);
                let (sequence_number, _status) =
                    world.process_at_rank(status.source_rank()).receive::<u32>();
                unsafe {
                    let mut bz_buffer: bzip2_sys::bz_stream = mem::zeroed();
                    bzip2_sys::BZ2_bzDecompressInit(&mut bz_buffer as *mut _, 0, 0);

                    let mut output: Vec<u8> = vec![0; BLOCK_SIZE];

                    bz_buffer.next_in = buf.as_ptr() as *mut _;
                    bz_buffer.avail_in = buf.len() as _;
                    bz_buffer.next_out = output.as_mut_ptr() as *mut _;
                    bz_buffer.avail_out = output.len() as _;

                    bzip2_sys::BZ2_bzDecompress(&mut bz_buffer as *mut _);
                    bzip2_sys::BZ2_bzDecompressEnd(&mut bz_buffer as *mut _);

                    comm.send(&bz_buffer.total_out_lo32.to_ne_bytes());
                    comm.send(&output[0..bz_buffer.total_out_lo32 as usize]);
                    comm.send(&sequence_number.to_ne_bytes());
                }
            }
        }
    }
}
