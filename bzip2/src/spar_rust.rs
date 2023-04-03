use std::fs::File;
use std::io::{prelude::*, BufWriter};
use std::mem;
use std::time::SystemTime;

use rust_spp::*;
use spar_rust::to_stream;

use crate::BLOCK_SIZE;

pub fn spar_rust(threads: usize, file_action: &str, file_name: &str) {
    let mut file = File::open(file_name).expect("No file found.");
    let mut buffer_input = vec![];
    let mut buffer_output = vec![];
    file.read_to_end(&mut buffer_input).unwrap();

    if file_action == "compress" {
        let compressed_file_name = file_name.to_owned() + ".bz2";
        let outfile = File::create(compressed_file_name).unwrap();
        let mut buf_write = BufWriter::new(outfile);

        // initialization
        let mut pos_init: usize;
        let mut pos_end = 0;
        let mut bytes_left = buffer_input.len();

        let start = SystemTime::now();

        to_stream!(INPUT(buffer_output: Vec<u8>), {
            while bytes_left > 0 {
                pos_init = pos_end;
                pos_end += if bytes_left < BLOCK_SIZE {
                    buffer_input.len() - pos_end
                } else {
                    BLOCK_SIZE
                };
                bytes_left -= pos_end - pos_init;

                let buffer_slice = &buffer_input[pos_init..pos_end];
                let buffer_input = buffer_slice.to_vec();

                STAGE(
                    INPUT(buffer_input: Vec<u8>),
                    OUTPUT(output: Vec<u8>, size: usize),
                    REPLICATE = threads,
                    {
                        let mut bz_buffer: bzip2_sys::bz_stream;
                        unsafe {
                            bz_buffer = mem::zeroed();
                            bzip2_sys::BZ2_bzCompressInit(&mut bz_buffer as *mut _, 9, 0, 30);
                        }

                        let mut output: Vec<u8> =
                            vec![0; (buffer_input.len() as f64 * 1.01) as usize + 600];

                        bz_buffer.next_in = buffer_input.as_ptr() as *mut _;
                        bz_buffer.avail_in = buffer_input.len() as _;
                        bz_buffer.next_out = output.as_mut_ptr() as *mut _;
                        bz_buffer.avail_out = output.len() as _;

                        unsafe {
                            bzip2_sys::BZ2_bzCompress(
                                &mut bz_buffer as *mut _,
                                bzip2_sys::BZ_FINISH as _,
                            );
                            bzip2_sys::BZ2_bzCompressEnd(&mut bz_buffer as *mut _);
                        }
                        let size = bz_buffer.total_out_lo32 as usize;
                    },
                );
                // write stage
                STAGE(
                    INPUT(output: Vec<u8>, size: usize, buffer_output: Vec<u8>),
                    ORDERED,
                    {
                        buffer_output.extend(&output[0..size]);
                    },
                );
            }
        });

        let system_duration = start.elapsed().expect("Failed to get render time?");
        let in_sec =
            system_duration.as_secs() as f64 + system_duration.subsec_nanos() as f64 * 1e-9;
        println!("Execution time: {in_sec} sec");

        // write compressed data to file
        buf_write.write_all(&buffer_output).unwrap();
        std::fs::remove_file(file_name).unwrap();
    } else if file_action == "decompress" {
        // creating the decompressed file
        let decompressed_file_name = &file_name.to_owned()[..file_name.len() - 4];
        let outfile = File::create(decompressed_file_name).unwrap();
        let mut buf_write = BufWriter::new(outfile);
        let mut buffer_output = vec![];

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

        to_stream!(INPUT(buffer_output: Vec<u8>), {
            for block in queue_blocks {
                let buffer_slice = &buffer_input[block.0..block.1];
                let buffer_input = buffer_slice.to_vec();

                STAGE(
                    INPUT(buffer_input: Vec<u8>),
                    OUTPUT(output: Vec<u8>, size: usize),
                    REPLICATE = threads,
                    {
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
                    },
                );
                STAGE(
                    INPUT(output: Vec<u8>, size: usize, buffer_output: Vec<u8>),
                    ORDERED,
                    {
                        // write stage
                        buffer_output.extend(&output[0..size]);
                    },
                );
            }
        });

        let system_duration = start.elapsed().expect("Failed to get render time?");
        let in_sec =
            system_duration.as_secs() as f64 + system_duration.subsec_nanos() as f64 * 1e-9;
        println!("Execution time: {in_sec} sec");

        // write decompressed data to file
        buf_write.write_all(&buffer_output).unwrap();
        std::fs::remove_file(file_name).unwrap();
    }
}

pub fn spar_rust_io(threads: usize, file_action: &str, file_name: &str) {
    let mut file = File::open(file_name).expect("No file found.");

    if file_action == "compress" {
        let compressed_file_name = file_name.to_owned() + ".bz2";

        // initialization
        let mut pos_init: usize;
        let mut pos_end = 0;
        let mut bytes_left: usize = file.metadata().unwrap().len() as usize;

        let start = SystemTime::now();

        let filename = compressed_file_name;
        to_stream!(INPUT(filename: String), {
            while bytes_left > 0 {
                pos_init = pos_end;
                pos_end += if bytes_left < BLOCK_SIZE {
                    file.metadata().unwrap().len() as usize - pos_end
                } else {
                    BLOCK_SIZE
                };
                bytes_left -= pos_end - pos_init;

                //let buffer_slice = &buffer_input[pos_init..pos_end];
                let mut buffer: Vec<u8> = vec![0; pos_end - pos_init];
                file.read_exact(&mut buffer).unwrap();

                // computation
                STAGE(
                    INPUT(buffer: Vec<u8>),
                    OUTPUT(output: Vec<u8>, size: usize),
                    REPLICATE = threads,
                    {
                        let mut bz_buffer: bzip2_sys::bz_stream;
                        unsafe {
                            bz_buffer = mem::zeroed();
                            bzip2_sys::BZ2_bzCompressInit(&mut bz_buffer as *mut _, 9, 0, 30);
                        }
                        let mut output: Vec<u8> =
                            vec![0; (buffer.len() as f64 * 1.01) as usize + 600];

                        bz_buffer.next_in = buffer.as_ptr() as *mut _;
                        bz_buffer.avail_in = buffer.len() as _;
                        bz_buffer.next_out = output.as_mut_ptr() as *mut _;
                        bz_buffer.avail_out = output.len() as _;

                        unsafe {
                            bzip2_sys::BZ2_bzCompress(
                                &mut bz_buffer as *mut _,
                                bzip2_sys::BZ_FINISH as _,
                            );
                            bzip2_sys::BZ2_bzCompressEnd(&mut bz_buffer as *mut _);
                        }
                        let size = bz_buffer.total_out_lo32 as usize;
                    },
                );
                // write stage
                STAGE(
                    INPUT(output: Vec<u8>, size: usize, filename: String),
                    ORDERED,
                    {
                        let file = File::options()
                            .create(true)
                            .append(true)
                            .open(filename)
                            .unwrap();
                        let mut buf_write = BufWriter::new(file);
                        buf_write.write_all(&output[0..size]).unwrap();
                    },
                );
            }
        });

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
        let filename = decompressed_file_name.to_string();
        to_stream!(INPUT(filename: String), {
            for block in queue_blocks {
                let buffer_slice = &buffer_input[block.0..block.1];
                let buffer = buffer_slice.to_vec();

                // computation
                STAGE(
                    INPUT(buffer: Vec<u8>),
                    OUTPUT(output: Vec<u8>, size: usize),
                    REPLICATE = threads,
                    {
                        let mut bz_buffer: bzip2_sys::bz_stream;
                        let mut output: Vec<u8> = vec![0; BLOCK_SIZE];
                        unsafe {
                            bz_buffer = mem::zeroed();
                            bzip2_sys::BZ2_bzDecompressInit(&mut bz_buffer as *mut _, 0, 0);

                            bz_buffer.next_in = buffer.as_ptr() as *mut _;
                            bz_buffer.avail_in = buffer.len() as _;
                            bz_buffer.next_out = output.as_mut_ptr() as *mut _;
                            bz_buffer.avail_out = output.len() as _;

                            bzip2_sys::BZ2_bzDecompress(&mut bz_buffer as *mut _);
                            bzip2_sys::BZ2_bzDecompressEnd(&mut bz_buffer as *mut _);
                        }
                        let size = bz_buffer.total_out_lo32 as usize;
                    },
                );
                STAGE(
                    INPUT(output: Vec<u8>, size: usize, filename: String),
                    ORDERED,
                    {
                        let file = File::options()
                            .create(true)
                            .append(true)
                            .open(filename)
                            .unwrap();
                        let mut buf_write = BufWriter::new(file);
                        buf_write.write_all(&output[0..size]).unwrap();
                    },
                );
            }
        });

        let system_duration = start.elapsed().expect("Failed to get render time?");
        let in_sec =
            system_duration.as_secs() as f64 + system_duration.subsec_nanos() as f64 * 1e-9;
        println!("Execution time: {in_sec} sec");

        std::fs::remove_file(file_name).unwrap();
    }
}
