use std::fs::File;
use std::io::prelude::*;
use std::mem;
use std::time::SystemTime;

use crate::BLOCK_SIZE;

pub fn sequential(file_action: &str, file_name: &str) {
    let mut file = File::open(file_name).expect("No file found.");

    if file_action == "compress" {
        let compressed_file_name = file_name.to_owned() + ".bz2";
        let mut outfile = File::create(compressed_file_name).unwrap();
        let mut buffer_input = vec![];
        let mut buffer_output = vec![];

        // read data to memory
        file.read_to_end(&mut buffer_input).unwrap();

        // initialization
        let start = SystemTime::now();
        (0..buffer_input.len()).step_by(BLOCK_SIZE).for_each(|i| {
            let buffer_slice = if i + BLOCK_SIZE >= buffer_input.len() {
                &buffer_input[i..]
            } else {
                &buffer_input[i..i + BLOCK_SIZE]
            };
            unsafe {
                let mut bz_buffer: bzip2_sys::bz_stream = mem::zeroed();
                bzip2_sys::BZ2_bzCompressInit(&mut bz_buffer as *mut _, 9, 0, 30);

                let mut output = vec![0u8; (buffer_slice.len() as f64 * 1.01) as usize + 600];

                bz_buffer.next_in = buffer_slice.as_ptr() as *mut _;
                bz_buffer.avail_in = buffer_slice.len() as _;
                bz_buffer.next_out = output.as_mut_ptr() as *mut _;
                bz_buffer.avail_out = output.len() as _;

                bzip2_sys::BZ2_bzCompress(&mut bz_buffer as *mut _, bzip2_sys::BZ_FINISH as _);
                bzip2_sys::BZ2_bzCompressEnd(&mut bz_buffer as *mut _);

                // write stage
                buffer_output.extend(&output[0..bz_buffer.total_out_lo32 as usize]);
            }
        });

        let system_duration = start.elapsed().expect("Failed to get render time?");
        let in_sec =
            system_duration.as_secs() as f64 + system_duration.subsec_nanos() as f64 * 1e-9;
        println!("Execution time: {in_sec} sec");

        // write compressed data to file
        outfile.write_all(&buffer_output).unwrap();
        std::fs::remove_file(file_name).unwrap();
    } else if file_action == "decompress" {
        // creating the decompressed file
        let decompressed_file_name = &file_name.to_owned()[..file_name.len() - 4];
        let mut outfile = File::create(decompressed_file_name).unwrap();
        let mut buffer_input = vec![];
        let mut buffer_output = vec![];

        // read data to memory
        file.read_to_end(&mut buffer_input).unwrap();

        // initialization
        let mut pos_init: usize;
        let mut pos_end = 0;
        let mut bytes_left = buffer_input.len();
        let mut queue_blocks = Vec::new();

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
        for block in queue_blocks {
            let buffer_slice = &buffer_input[block.0..block.1];

            // computation
            unsafe {
                let mut bz_buffer: bzip2_sys::bz_stream = mem::zeroed();
                bzip2_sys::BZ2_bzDecompressInit(&mut bz_buffer as *mut _, 0, 0);

                let mut output = vec![0; BLOCK_SIZE];

                bz_buffer.next_in = buffer_slice.as_ptr() as *mut _;
                bz_buffer.avail_in = buffer_slice.len() as _;
                bz_buffer.next_out = output.as_mut_ptr() as *mut _;
                bz_buffer.avail_out = output.len() as _;

                bzip2_sys::BZ2_bzDecompress(&mut bz_buffer as *mut _);
                bzip2_sys::BZ2_bzDecompressEnd(&mut bz_buffer as *mut _);

                // write stage
                buffer_output.extend(&output[0..bz_buffer.total_out_lo32 as usize]);
            }
        }

        let system_duration = start.elapsed().expect("Failed to get render time?");
        let in_sec =
            system_duration.as_secs() as f64 + system_duration.subsec_nanos() as f64 * 1e-9;
        println!("Execution time: {in_sec} sec");

        // write decompressed data to file
        outfile.write_all(&buffer_output).unwrap();
        std::fs::remove_file(file_name).unwrap();
    }
}

pub fn sequential_io(file_action: &str, file_name: &str) {
    let mut file = File::open(file_name).expect("No file found.");

    if file_action == "compress" {
        let compressed_file_name = file_name.to_owned() + ".bz2";
        let mut outfile = File::create(compressed_file_name).unwrap();

        // initialization
        let start = SystemTime::now();

        let file_size = file.metadata().unwrap().len() as usize;
        let mut buffer = Vec::new();
        (0..file_size).step_by(BLOCK_SIZE).for_each(|i| {
            if i + BLOCK_SIZE >= file_size {
                buffer.resize(file_size - i, 0);
            } else {
                buffer.resize(BLOCK_SIZE, 0);
            }
            file.read_exact(&mut buffer).unwrap();

            unsafe {
                let mut bz_buffer: bzip2_sys::bz_stream = mem::zeroed();
                bzip2_sys::BZ2_bzCompressInit(&mut bz_buffer as *mut _, 9, 0, 30);

                let mut output = vec![0; (buffer.len() as f64 * 1.01) as usize + 600];

                bz_buffer.next_in = buffer.as_ptr() as *mut _;
                bz_buffer.avail_in = buffer.len() as _;
                bz_buffer.next_out = output.as_mut_ptr() as *mut _;
                bz_buffer.avail_out = output.len() as _;

                bzip2_sys::BZ2_bzCompress(&mut bz_buffer as *mut _, bzip2_sys::BZ_FINISH as _);
                bzip2_sys::BZ2_bzCompressEnd(&mut bz_buffer as *mut _);

                // write stage
                outfile
                    .write_all(&output[0..bz_buffer.total_out_lo32 as usize])
                    .unwrap();
            }
        });

        let system_duration = start.elapsed().expect("Failed to get render time?");
        let in_sec =
            system_duration.as_secs() as f64 + system_duration.subsec_nanos() as f64 * 1e-9;
        println!("Execution time: {in_sec} sec");

        // write compressed data to file
        std::fs::remove_file(file_name).unwrap();
    } else if file_action == "decompress" {
        // creating the decompressed file
        let decompressed_file_name = &file_name.to_owned()[..file_name.len() - 4];
        let mut outfile = File::create(decompressed_file_name).unwrap();
        let mut buffer_input = vec![];

        // read data to memory
        file.read_to_end(&mut buffer_input).unwrap();

        // initialization
        let mut pos_init: usize;
        let mut pos_end = 0;
        let mut bytes_left = buffer_input.len();
        let mut queue_blocks = Vec::new();

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
        for block in queue_blocks {
            let buffer_slice = &buffer_input[block.0..block.1];

            // computation
            unsafe {
                let mut bz_buffer: bzip2_sys::bz_stream = mem::zeroed();
                bzip2_sys::BZ2_bzDecompressInit(&mut bz_buffer as *mut _, 0, 0);

                let mut output = vec![0; BLOCK_SIZE];

                bz_buffer.next_in = buffer_slice.as_ptr() as *mut _;
                bz_buffer.avail_in = buffer_slice.len() as _;
                bz_buffer.next_out = output.as_mut_ptr() as *mut _;
                bz_buffer.avail_out = output.len() as _;

                bzip2_sys::BZ2_bzDecompress(&mut bz_buffer as *mut _);
                bzip2_sys::BZ2_bzDecompressEnd(&mut bz_buffer as *mut _);

                // write stage
                outfile
                    .write_all(&output[0..bz_buffer.total_out_lo32 as usize])
                    .unwrap();
            }
        }

        let system_duration = start.elapsed().expect("Failed to get render time?");
        let in_sec =
            system_duration.as_secs() as f64 + system_duration.subsec_nanos() as f64 * 1e-9;
        println!("Execution time: {in_sec} sec");

        // write decompressed data to file
        std::fs::remove_file(file_name).unwrap();
    }
}
