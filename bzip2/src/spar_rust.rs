use std::fs::File;
use std::io::prelude::*;
use std::mem;
use std::time::SystemTime;

use rust_spp::*;
use spar_rust::to_stream;

struct Content {
    buffer_output: Vec<u8>,
    output_size: u32,
}

pub fn spar_rust(threads: usize, file_action: &str, file_name: &str) {
    let mut file = File::open(file_name).expect("No file found.");
    let mut buffer_input = vec![];
    file.read_to_end(&mut buffer_input).unwrap();

    if file_action == "compress" {
        let compressed_file_name = file_name.to_owned() + ".bz2";
        let mut buf_write = File::create(compressed_file_name).unwrap();

        // initialization
        let block_size = 900000;
        let mut pos_init: usize;
        let mut pos_end = 0;
        let mut bytes_left = buffer_input.len();

        let start = SystemTime::now();

        let collection = to_stream!(OUTPUT(Content), {
            while bytes_left > 0 {
                pos_init = pos_end;
                pos_end += if bytes_left < block_size {
                    buffer_input.len() - pos_end
                } else {
                    block_size
                };
                bytes_left -= pos_end - pos_init;

                let buffer_slice = &buffer_input[pos_init..pos_end];
                let buffer_input = buffer_slice.to_vec();
                let buffer_output = vec![0; (buffer_slice.len() as f64 * 1.01) as usize + 600];

                STAGE(
                    INPUT(buffer_input: Vec<u8>, buffer_output: Vec<u8>),
                    OUTPUT(Content),
                    REPLICATE = threads,
                    {
                        let content;
                        unsafe {
                            // computation
                            let mut bz_buffer: bzip2_sys::bz_stream = mem::zeroed();
                            bzip2_sys::BZ2_bzCompressInit(&mut bz_buffer as *mut _, 9, 0, 30);

                            bz_buffer.next_in = buffer_input.as_ptr() as *mut _;
                            bz_buffer.avail_in = buffer_input.len() as _;
                            bz_buffer.next_out = buffer_output.as_mut_ptr() as *mut _;
                            bz_buffer.avail_out = buffer_output.len() as _;

                            bzip2_sys::BZ2_bzCompress(
                                &mut bz_buffer as *mut _,
                                bzip2_sys::BZ_FINISH as _,
                            );
                            bzip2_sys::BZ2_bzCompressEnd(&mut bz_buffer as *mut _);

                            content = Content {
                                buffer_output,
                                output_size: bz_buffer.total_out_lo32,
                            };
                        }
                        Some(content)
                    },
                );
            }
        });

        let system_duration = start.elapsed().expect("Failed to get render time?");
        let in_sec =
            system_duration.as_secs() as f64 + system_duration.subsec_nanos() as f64 * 1e-9;
        println!("Execution time: {} sec", in_sec);

        // write stage
        let mut buffer_output: Vec<u8> = Vec::new();
        for content in collection {
            buffer_output.extend(&content.buffer_output[0..content.output_size as usize]);
        }
        // write compressed data to file
        buf_write.write_all(&buffer_output).unwrap();
        std::fs::remove_file(file_name).unwrap();
    } else if file_action == "decompress" {
        // creating the decompressed file
        let decompressed_file_name = &file_name.to_owned()[..file_name.len() - 4];
        let mut buf_write = File::create(decompressed_file_name).unwrap();
        let mut buffer_output = vec![];

        // initialization
        let block_size = 900000;
        let mut pos_init: usize;
        let mut pos_end = 0;
        let mut bytes_left = buffer_input.len();
        let mut queue_blocks: Vec<(usize, usize)> = Vec::new();

        while bytes_left > 0 {
            pos_init = pos_end;
            pos_end += {
                // find the ending position by identifing the header of the next stream block
                let buffer_slice;
                if buffer_input.len() > block_size + 10000 {
                    if (pos_init + block_size + 10000) > buffer_input.len() {
                        buffer_slice = &buffer_input[pos_init + 10..];
                    } else {
                        buffer_slice = &buffer_input[pos_init + 10..pos_init + block_size + 10000];
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

        let collection = to_stream!(OUTPUT(Content), {
            for block in queue_blocks {
                let buffer_slice = &buffer_input[block.0..block.1];
                let buffer_input = buffer_slice.to_vec();
                let buffer_output = vec![0; block_size];
                STAGE(
                    INPUT(buffer_input: Vec<u8>, buffer_output: Vec<u8>),
                    OUTPUT(Content),
                    REPLICATE = threads,
                    {
                        let content;

                        unsafe {
                            // computation
                            let mut bz_buffer: bzip2_sys::bz_stream = mem::zeroed();
                            bzip2_sys::BZ2_bzDecompressInit(&mut bz_buffer as *mut _, 0, 0);

                            bz_buffer.next_in = buffer_input.as_ptr() as *mut _;
                            bz_buffer.avail_in = buffer_input.len() as _;
                            bz_buffer.next_out = buffer_output.as_mut_ptr() as *mut _;
                            bz_buffer.avail_out = buffer_output.len() as _;

                            bzip2_sys::BZ2_bzDecompress(&mut bz_buffer as *mut _);
                            bzip2_sys::BZ2_bzDecompressEnd(&mut bz_buffer as *mut _);

                            content = Content {
                                buffer_output,
                                output_size: bz_buffer.total_out_lo32,
                            };
                        }

                        Some(content)
                    },
                );
            }
        });

        let system_duration = start.elapsed().expect("Failed to get render time?");
        let in_sec =
            system_duration.as_secs() as f64 + system_duration.subsec_nanos() as f64 * 1e-9;
        println!("Execution time: {} sec", in_sec);

        // write stage
        for content in collection {
            buffer_output.extend(&content.buffer_output[0..content.output_size as usize]);
        }

        // write decompressed data to file
        buf_write.write_all(&buffer_output).unwrap();
        std::fs::remove_file(file_name).unwrap();
    }
}

pub fn spar_rust_io(threads: usize, file_action: &str, file_name: &str) {
    let mut file = File::open(file_name).expect("No file found.");

    if file_action == "compress" {
        // initialization
        let block_size = 900000;
        let mut pos_init: usize;
        let mut pos_end = 0;
        let mut bytes_left: usize = file.metadata().unwrap().len() as usize;

        let start = SystemTime::now();

        let collection = to_stream!(OUTPUT(Content), {
            while bytes_left > 0 {
                pos_init = pos_end;
                pos_end += if bytes_left < block_size {
                    file.metadata().unwrap().len() as usize - pos_end
                } else {
                    block_size
                };
                bytes_left -= pos_end - pos_init;

                let mut buffer_slice: Vec<u8> = vec![0; pos_end - pos_init];
                file.read_exact(&mut buffer_slice).unwrap();
                let buffer_input = buffer_slice.to_vec();
                let buffer_output = vec![0; (buffer_slice.len() as f64 * 1.01) as usize + 600];

                STAGE(
                    INPUT(buffer_input: Vec<u8>, buffer_output: Vec<u8>),
                    OUTPUT(Content),
                    REPLICATE = threads,
                    {
                        let content;
                        unsafe {
                            // computation
                            let mut bz_buffer: bzip2_sys::bz_stream = mem::zeroed();
                            bzip2_sys::BZ2_bzCompressInit(&mut bz_buffer as *mut _, 9, 0, 30);

                            bz_buffer.next_in = buffer_input.as_ptr() as *mut _;
                            bz_buffer.avail_in = buffer_input.len() as _;
                            bz_buffer.next_out = buffer_output.as_mut_ptr() as *mut _;
                            bz_buffer.avail_out = buffer_output.len() as _;

                            bzip2_sys::BZ2_bzCompress(
                                &mut bz_buffer as *mut _,
                                bzip2_sys::BZ_FINISH as _,
                            );
                            bzip2_sys::BZ2_bzCompressEnd(&mut bz_buffer as *mut _);

                            content = Content {
                                buffer_output,
                                output_size: bz_buffer.total_out_lo32,
                            };
                        }
                        Some(content)
                    },
                );
            }
        });

        let compressed_file_name = file_name.to_owned() + ".bz2";
        let mut compressed_file = File::create(compressed_file_name).unwrap();
        for content in collection {
            compressed_file
                .write_all(&content.buffer_output[0..content.output_size as usize])
                .unwrap();
        }

        let system_duration = start.elapsed().expect("Failed to get render time?");
        let in_sec =
            system_duration.as_secs() as f64 + system_duration.subsec_nanos() as f64 * 1e-9;
        println!("Execution time: {} sec", in_sec);

        std::fs::remove_file(file_name).unwrap();
    } else if file_action == "decompress" {
        let mut buffer_input = vec![];

        // read data to memory
        file.read_to_end(&mut buffer_input).unwrap();

        // initialization
        let block_size = 900000;
        let mut pos_init: usize;
        let mut pos_end = 0;
        let mut bytes_left = buffer_input.len();
        let mut queue_blocks: Vec<(usize, usize)> = Vec::new();

        while bytes_left > 0 {
            pos_init = pos_end;
            pos_end += {
                // find the ending position by identifing the header of the next stream block
                let buffer_slice;
                if buffer_input.len() > block_size + 10000 {
                    if (pos_init + block_size + 10000) > buffer_input.len() {
                        buffer_slice = &buffer_input[pos_init + 10..];
                    } else {
                        buffer_slice = &buffer_input[pos_init + 10..pos_init + block_size + 10000];
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

        let collection = to_stream!(OUTPUT(Content), {
            for block in queue_blocks {
                let buffer_slice = &buffer_input[block.0..block.1];
                let buffer_input = buffer_slice.to_vec();
                let buffer_output = vec![0; block_size];
                STAGE(
                    INPUT(buffer_input: Vec<u8>, buffer_output: Vec<u8>),
                    OUTPUT(Content),
                    REPLICATE = threads,
                    {
                        let content;

                        unsafe {
                            // computation
                            let mut bz_buffer: bzip2_sys::bz_stream = mem::zeroed();
                            bzip2_sys::BZ2_bzDecompressInit(&mut bz_buffer as *mut _, 0, 0);

                            bz_buffer.next_in = buffer_input.as_ptr() as *mut _;
                            bz_buffer.avail_in = buffer_input.len() as _;
                            bz_buffer.next_out = buffer_output.as_mut_ptr() as *mut _;
                            bz_buffer.avail_out = buffer_output.len() as _;

                            bzip2_sys::BZ2_bzDecompress(&mut bz_buffer as *mut _);
                            bzip2_sys::BZ2_bzDecompressEnd(&mut bz_buffer as *mut _);

                            content = Content {
                                buffer_output,
                                output_size: bz_buffer.total_out_lo32,
                            };
                        }

                        Some(content)
                    },
                );
            }
        });
        let decompressed_file_name = file_name.to_owned()[..file_name.len() - 4].to_owned();
        let mut decompressed_file = File::create(decompressed_file_name).unwrap();
        for content in collection {
            decompressed_file
                .write_all(&content.buffer_output[0..content.output_size as usize])
                .unwrap();
        }

        let system_duration = start.elapsed().expect("Failed to get render time?");
        let in_sec =
            system_duration.as_secs() as f64 + system_duration.subsec_nanos() as f64 * 1e-9;
        println!("Execution time: {} sec", in_sec);

        std::fs::remove_file(file_name).unwrap();
    }
}
