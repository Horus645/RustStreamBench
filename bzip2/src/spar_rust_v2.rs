use std::fs::File;
use std::io::{prelude::*, BufWriter};
use std::mem;
use std::time::SystemTime;

use spar_rust_v2::*;

use crate::BLOCK_SIZE;

#[source]
fn io_compress_source(mut file: File) -> impl Iterator<Item = Vec<u8>> {
    let file_size = file.metadata().unwrap().len() as usize;
    (0..file_size).step_by(BLOCK_SIZE).map(move |i| {
        let mut buffer = if i + BLOCK_SIZE >= file_size {
            vec![0; file_size - i]
        } else {
            vec![0; BLOCK_SIZE]
        };
        file.read_exact(&mut buffer).unwrap();
        buffer
    })
}

#[source]
fn in_mem_compress_source(buffer_input: Vec<u8>) -> impl Iterator<Item = Vec<u8>> {
    (0..buffer_input.len()).step_by(BLOCK_SIZE).map(move |i| {
        if i + BLOCK_SIZE >= buffer_input.len() {
            buffer_input[i..].to_vec()
        } else {
            buffer_input[i..i + BLOCK_SIZE].to_vec()
        }
    })
}

#[source]
fn in_mem_decompress_source(
    buffer_input: Vec<u8>,
    queue_blocks: Vec<(usize, usize)>,
) -> impl Iterator<Item = Vec<u8>> {
    queue_blocks
        .into_iter()
        .map(move |(start, end)| buffer_input[start..end].to_vec())
}

#[stage]
fn compress_stage(buffer_input: Vec<u8>) -> (Vec<u8>, usize) {
    let mut bz_buffer: bzip2_sys::bz_stream;
    unsafe {
        bz_buffer = mem::zeroed();
        bzip2_sys::BZ2_bzCompressInit(&mut bz_buffer as *mut _, 9, 0, 30);
    }

    let mut output: Vec<u8> = vec![0; (buffer_input.len() as f64 * 1.01) as usize + 600];

    bz_buffer.next_in = buffer_input.as_ptr() as *mut _;
    bz_buffer.avail_in = buffer_input.len() as _;
    bz_buffer.next_out = output.as_mut_ptr() as *mut _;
    bz_buffer.avail_out = output.len() as _;

    unsafe {
        bzip2_sys::BZ2_bzCompress(&mut bz_buffer as *mut _, bzip2_sys::BZ_FINISH as _);
        bzip2_sys::BZ2_bzCompressEnd(&mut bz_buffer as *mut _);
    }
    let size = bz_buffer.total_out_lo32 as usize;

    (output, size)
}

#[stage]
fn decompress_stage(buffer_input: Vec<u8>) -> (Vec<u8>, usize) {
    // computation
    let mut bz_buffer: bzip2_sys::bz_stream;
    unsafe {
        bz_buffer = mem::zeroed();
        bzip2_sys::BZ2_bzDecompressInit(&mut bz_buffer as *mut _, 0, 0);
    }
    let mut output: Vec<u8> = vec![0; BLOCK_SIZE];

    bz_buffer.next_in = buffer_input.as_ptr() as *mut _;
    bz_buffer.avail_in = buffer_input.len() as _;
    bz_buffer.next_out = output.as_mut_ptr() as *mut _;
    bz_buffer.avail_out = output.len() as _;
    unsafe {
        bzip2_sys::BZ2_bzDecompress(&mut bz_buffer as *mut _);
        bzip2_sys::BZ2_bzDecompressEnd(&mut bz_buffer as *mut _);
    }
    let size = bz_buffer.total_out_lo32 as usize;

    (output, size)
}

static mut RAM_SINK_BUFFER_OUTPUT: Vec<u8> = Vec::new();
#[sink(Ordered)]
fn in_memory_sink(output: Vec<u8>, size: usize) {
    unsafe {
        RAM_SINK_BUFFER_OUTPUT.extend(&output[0..size]);
    }
}

static mut IO_SINK_FILE_OUTPUT: String = String::new();
#[sink(Ordered)]
fn disk_memory_sink(output: Vec<u8>, size: usize) {
    let file = File::options()
        .create(true)
        .append(true)
        .open(unsafe { &IO_SINK_FILE_OUTPUT })
        .unwrap();
    let mut buf_write = BufWriter::new(file);
    buf_write.write_all(&output[0..size]).unwrap();
}

pub fn spar_rust_v2(threads: usize, file_action: &str, file_name: &str) {
    let mut file = File::open(file_name).expect("No file found.");
    let mut buffer_input = Vec::with_capacity(1 << 10);
    file.read_to_end(&mut buffer_input).unwrap();

    if file_action == "compress" {
        let compressed_file_name = file_name.to_owned() + ".bz2";
        let outfile = File::create(compressed_file_name).unwrap();
        let mut buf_write = BufWriter::new(outfile);

        let start = SystemTime::now();

        to_stream!(multithreaded: [
            in_mem_compress_source, (buffer_input),
            (compress_stage, threads),
            in_memory_sink,
        ])
        .join()
        .unwrap();

        let system_duration = start.elapsed().expect("Failed to get render time?");
        let in_sec =
            system_duration.as_secs() as f64 + system_duration.subsec_nanos() as f64 * 1e-9;
        println!("Execution time: {in_sec} sec");

        // write compressed data to file
        buf_write
            .write_all(unsafe { &RAM_SINK_BUFFER_OUTPUT })
            .unwrap();
        std::fs::remove_file(file_name).unwrap();
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

        to_stream!(multithreaded: [
            in_mem_decompress_source, (buffer_input, queue_blocks),
            (decompress_stage, threads),
            in_memory_sink,
        ])
        .join()
        .unwrap();

        let system_duration = start.elapsed().expect("Failed to get render time?");
        let in_sec =
            system_duration.as_secs() as f64 + system_duration.subsec_nanos() as f64 * 1e-9;
        println!("Execution time: {in_sec} sec");

        // write decompressed data to file
        println!("{}", unsafe { &RAM_SINK_BUFFER_OUTPUT }.len());
        buf_write
            .write_all(unsafe { &RAM_SINK_BUFFER_OUTPUT })
            .unwrap();
        std::fs::remove_file(file_name).unwrap();
    }
}

pub fn spar_rust_v2_io(threads: usize, file_action: &str, file_name: &str) {
    let mut file = File::open(file_name).expect("No file found.");

    if file_action == "compress" {
        let compressed_file_name = file_name.to_owned() + ".bz2";

        // initialization

        let start = SystemTime::now();
        unsafe {
            IO_SINK_FILE_OUTPUT = compressed_file_name;
        }

        to_stream!(multithreaded: [
            io_compress_source, (file),
            (compress_stage, threads),
            disk_memory_sink
        ])
        .join()
        .unwrap();

        let system_duration = start.elapsed().expect("Failed to get render time?");
        let in_sec =
            system_duration.as_secs() as f64 + system_duration.subsec_nanos() as f64 * 1e-9;
        println!("Execution time: {in_sec} sec");

        std::fs::remove_file(file_name).unwrap();
    } else if file_action == "decompress" {
        let decompressed_file_name = &file_name.to_owned()[..file_name.len() - 4];
        let mut buffer_input = vec![];

        // read data to memory
        file.read_to_end(&mut buffer_input).unwrap();

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

        // Stream region
        unsafe {
            IO_SINK_FILE_OUTPUT = decompressed_file_name.to_string();
        }

        to_stream!(multithreaded: [
            in_mem_decompress_source, (buffer_input, queue_blocks),
            (decompress_stage, threads),
            disk_memory_sink,
        ])
        .join()
        .unwrap();

        let system_duration = start.elapsed().expect("Failed to get render time?");
        let in_sec =
            system_duration.as_secs() as f64 + system_duration.subsec_nanos() as f64 * 1e-9;
        println!("Execution time: {in_sec} sec");

        std::fs::remove_file(file_name).unwrap();
    }
}
