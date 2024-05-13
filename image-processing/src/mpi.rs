use raster::filter;
use serde::{de::Visitor, ser::SerializeStruct, Deserialize, Serialize};
use std::time::SystemTime;

use spar_rust_v2::mpi::{
    self,
    traits::{Communicator, Destination, Source},
};

#[derive(Debug)]
#[repr(transparent)]
struct Image(raster::Image);

impl Serialize for Image {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let raster::Image {
            width,
            height,
            bytes,
        } = &self.0;

        let mut state = serializer.serialize_struct("Image", 3)?;
        state.serialize_field("width", width)?;
        state.serialize_field("height", height)?;
        state.serialize_field("bytes", bytes)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for Image {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "lowercase")]
        enum Filed {
            Width,
            Height,
            Bytes,
        }

        struct ImageVisitor;

        impl<'de> Visitor<'de> for ImageVisitor {
            type Value = Image;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("struct Image")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let width = seq.next_element()?.unwrap();
                let height = seq.next_element()?.unwrap();
                let bytes = seq.next_element()?.unwrap();
                Ok(Image(raster::Image {
                    width,
                    height,
                    bytes,
                }))
            }
        }

        deserializer.deserialize_struct("Image", &["width", "height", "bytes"], ImageVisitor)
    }
}

pub fn rsmpi(dir_name: &str, threads: usize) {
    let dir_entries = std::fs::read_dir(dir_name);
    let mut all_images: Vec<Image> = Vec::new();

    for entry in dir_entries.unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();

        if path.extension().is_none() {
            continue;
        }
        all_images.push(Image(raster::open(path.to_str().unwrap()).unwrap()));
    }

    let start = SystemTime::now();

    let (universe, _threading) = mpi::initialize_with_threading(mpi::Threading::Multiple).unwrap();
    let world = universe.world();
    let size = world.size() as usize;
    let threads = 1 + threads * 5;

    if size < threads {
        panic!("trying to execute with {threads} workers, but only have {size}");
    }

    let rank = world.rank();

    if rank as usize >= threads {
        std::process::exit(unsafe { mpi::ffi::MPI_Finalize() });
    }

    if rank == 0 {
        let len = all_images.len();
        std::thread::spawn(move || {
            let comm = mpi::topology::SimpleCommunicator::world();
            let mut target_rank = 1;
            for image in all_images.into_iter() {
                let bytes = bincode::serialize(&image).unwrap();
                let size = bytes.len() as u32;
                let target = comm.process_at_rank(target_rank);
                target.send(&size.to_ne_bytes());
                target.send(&bytes);

                target_rank += 1;
                if target_rank as usize >= threads / 5 {
                    target_rank = 1;
                }
            }
            for i in 1..(1 + threads / 5) {
                let target = comm.process_at_rank(i as i32);
                target.send(&0u32.to_ne_bytes());
            }
        });

        let mut sink: Vec<Image> = Vec::with_capacity(len);
        let comm = world.any_process();
        for _ in 0..len {
            let (size, status) = comm.receive::<u32>();
            let mut buf = vec![0u8; size as usize];
            let _status = world
                .process_at_rank(status.source_rank())
                .receive_into(&mut buf);
            sink.push(bincode::deserialize(&buf).unwrap());
        }

        let system_duration = start.elapsed().expect("Failed to get render time?");
        let in_sec =
            system_duration.as_secs() as f64 + system_duration.subsec_nanos() as f64 * 1e-9;
        println!("Execution time: {in_sec} sec");
    } else if rank > 0 && rank as usize <= (threads / 5) {
        let begin = 1 + (threads / 5);
        let end = 2 * (threads / 5);

        let recver = world.any_process();
        let sender = mpi::topology::SimpleCommunicator::world();
        let mut target = begin;
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
            let mut img: Image = bincode::deserialize(&buf).unwrap();
            filter::saturation(&mut img.0, 0.2).unwrap();

            {
                let bytes = bincode::serialize(&img).unwrap();
                let size = bytes.len() as u32;
                let target = sender.process_at_rank(target as i32);
                target.send(&size.to_ne_bytes());
                target.send(&bytes);
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
    } else if rank as usize > threads / 5 && rank as usize <= 2 * (threads / 5) {
        let begin = 1 + 2 * (threads / 5);
        let end = 3 * (threads / 5);

        let recver = world.any_process();
        let sender = mpi::topology::SimpleCommunicator::world();
        let mut target = begin;
        let mut zeros = threads / 5;
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
            let mut img: Image = bincode::deserialize(&buf).unwrap();
            filter::emboss(&mut img.0).unwrap();

            {
                let bytes = bincode::serialize(&img).unwrap();
                let size = bytes.len() as u32;
                let target = sender.process_at_rank(target as i32);
                target.send(&size.to_ne_bytes());
                target.send(&bytes);
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
    } else if rank as usize > 2 * (threads / 5) && rank as usize <= 3 * (threads / 5) {
        let begin = 1 + 3 * (threads / 5);
        let end = 4 * (threads / 5);

        let recver = world.any_process();
        let sender = mpi::topology::SimpleCommunicator::world();
        let mut target = begin;
        let mut zeros = threads / 5;
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
            let mut img: Image = bincode::deserialize(&buf).unwrap();
            filter::gamma(&mut img.0, 2.0).unwrap();

            {
                let bytes = bincode::serialize(&img).unwrap();
                let size = bytes.len() as u32;
                let target = sender.process_at_rank(target as i32);
                target.send(&size.to_ne_bytes());
                target.send(&bytes);
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
    } else if rank as usize > 3 * (threads / 5) && rank as usize <= 4 * (threads / 5) {
        let begin = 1 + 4 * (threads / 5);
        let end = 5 * (threads / 5);

        let recver = world.any_process();
        let sender = mpi::topology::SimpleCommunicator::world();
        let mut target = begin;
        let mut zeros = threads / 5;
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
            let mut img: Image = bincode::deserialize(&buf).unwrap();
            filter::sharpen(&mut img.0).unwrap();

            {
                let bytes = bincode::serialize(&img).unwrap();
                let size = bytes.len() as u32;
                let target = sender.process_at_rank(target as i32);
                target.send(&size.to_ne_bytes());
                target.send(&bytes);
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
        let mut zeros = threads / 5;
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
            let mut img: Image = bincode::deserialize(&buf).unwrap();
            filter::grayscale(&mut img.0).unwrap();

            {
                let bytes = bincode::serialize(&img).unwrap();
                let size = bytes.len() as u32;
                let target = sender.process_at_rank(target);
                target.send(&size.to_ne_bytes());
                target.send(&bytes);
            }
        }
    }
}
